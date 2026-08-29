import { useCallback, useEffect, useState } from "react";
import { api, errorText, events } from "../lib/api";
import type {
  BootCheck, BootCheckRequest, BootReport, CheckLevel, ReadbackResult, WriteProgress,
} from "../types";
import { bytes, pct } from "../lib/format";
import { Empty, Note, Panel, Progress } from "./ui";

const MARK: Record<CheckLevel, string> = { pass: "✓", warn: "!", fail: "✕", skipped: "?" };

const LEVEL_NAME: Record<CheckLevel, string> = {
  pass: "Đạt",
  warn: "Cần biết",
  fail: "Không đạt",
  skipped: "Không đọc được",
};

function CheckRow({ c }: { c: BootCheck }) {
  return (
    <div className="check">
      <span className="check__dot" data-s={c.level} title={LEVEL_NAME[c.level]}>
        {MARK[c.level]}
      </span>
      <div className="check__body">
        <div className="check__head">
          <span className="check__label">{c.label}</span>
          <span className="check__req">{c.expectation}</span>
        </div>
        <div className="check__value">{c.value}</div>
        {c.hint && <div className="check__hint">{c.hint}</div>}
      </div>
    </div>
  );
}

/** Hai đường khởi động là hai chuyện riêng: thiếu một đường không phải là hỏng. */
function BootWays({ r }: { r: BootReport }) {
  const ways: { on: boolean; title: string; desc: string }[] = [
    {
      on: r.bootable_uefi,
      title: "Máy UEFI đời mới",
      desc: "Phần lớn máy sản xuất từ 2012 trở lại đây",
    },
    {
      on: r.bootable_legacy,
      title: "Máy BIOS đời cũ / CSM",
      desc: "Máy đời cũ, hoặc máy mới bật chế độ tương thích",
    },
  ];
  return (
    <div className="grid grid--2">
      {ways.map((w) => (
        <div key={w.title} className="bootway" data-on={w.on}>
          <span className="bootway__mark">{w.on ? "✓" : "✕"}</span>
          <span>
            <span className="bootway__title">{w.title}</span>
            <span className="bootway__desc">{w.on ? w.desc : "Không khởi động được từ ổ này"}</span>
          </span>
        </div>
      ))}
    </div>
  );
}

export function StepVerify({
  request,
  writeDone,
  isoDiscarded,
}: {
  request: BootCheckRequest | null;
  /** Chưa ghi xong thì chưa có gì để kiểm tra. */
  writeDone: boolean;
  /**
   * File ISO đã bị dọn sau khi ghi. Kiểm tra cấu trúc vẫn chạy được, nhưng đối
   * chiếu từng byte thì cần chính file đó nên phải nói rõ thay vì để người dùng
   * bấm rồi nhận lỗi khó hiểu.
   */
  isoDiscarded: boolean;
}) {
  const [report, setReport] = useState<BootReport | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [readback, setReadback] = useState<ReadbackResult | null>(null);
  const [reading, setReading] = useState(false);
  const [prog, setProg] = useState<WriteProgress | null>(null);

  const key = request ? `${request.disk_number}|${request.iso_path}` : "";

  const run = useCallback(async () => {
    if (!request) return;
    setChecking(true);
    setError(null);
    try {
      setReport(await api.checkUsbBoot(request));
    } catch (e) {
      setError(errorText(e));
      setReport(null);
    } finally {
      setChecking(false);
    }
  }, [request]);

  // Phần kiểm tra cấu trúc chỉ mất vài giây nên chạy ngay khi mở bước này.
  // Phần đọc lại thì không — nó mất gần bằng thời gian ghi, và bắt người dùng
  // chờ thêm chừng đó mà không hỏi là quá đáng.
  useEffect(() => {
    setReport(null);
    setReadback(null);
    setProg(null);
    if (writeDone && key) void run();
    // `key` đổi nghĩa là đang kiểm tra một chiếc USB khác.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, writeDone]);

  async function startReadback() {
    if (!request) return;
    setReading(true);
    setError(null);
    setReadback(null);
    setProg(null);
    try {
      const un = await events.onVerifyProgress(setProg);
      try {
        setReadback(await api.verifyUsbReadback(request));
      } finally {
        un();
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setReading(false);
    }
  }

  if (!writeDone) {
    return (
      <>
        <div className="main__head"><h1>Kiểm tra khởi động</h1></div>
        <Note type="warn" icon="!" title="Chưa ghi xong">
          Hãy hoàn tất bước ghi trước. Bước này đọc lại chính chiếc USB vừa ghi để xác nhận
          nó khởi động được.
        </Note>
      </>
    );
  }

  const groups: string[] = [];
  for (const c of report?.checks ?? []) if (!groups.includes(c.group)) groups.push(c.group);

  const noteType = !report
    ? "info"
    : report.verdict === "ready" ? "ok"
    : report.verdict === "not_bootable" ? "danger" : "warn";

  return (
    <>
      <div className="main__head">
        <h1>Kiểm tra khởi động</h1>
        <p>
          Ghi xong không đồng nghĩa với boot được. Bước này đọc lại chiếc USB vừa ghi và đối
          chiếu với những gì firmware sẽ đi tìm lúc khởi động.
        </p>
      </div>

      {error && <Note type="danger" icon="✕" title="Không kiểm tra được">{error}</Note>}

      {!report && (
        <Panel>
          <Empty icon="🔍" title={checking ? "Đang kiểm tra…" : "Chưa kiểm tra"}>
            {checking
              ? "Đang đọc cấu trúc phân vùng và các file khởi động trên USB."
              : "Bấm nút bên dưới để kiểm tra lại."}
          </Empty>
          <div className="actions">
            <button className="btn btn--sm" onClick={run} disabled={checking}>
              {checking && <span className="spinner" />} Kiểm tra lại
            </button>
          </div>
        </Panel>
      )}

      {report && (
        <>
          <Note type={noteType} icon={report.verdict === "ready" ? "✓" : "◈"} title="Kết luận">
            {report.summary}
          </Note>

          <Panel title="Khởi động được trên">
            <BootWays r={report} />
            {/* Ổ có đủ mã khởi động nhưng thiếu file cài đặt vẫn boot bình
                thường rồi mới hỏng. Không nói rõ thì hai dấu tích xanh ở trên
                trông như đang cãi lại kết luận đỏ ngay phía trên chúng. */}
            {report.verdict === "not_bootable" && (report.bootable_uefi || report.bootable_legacy) && (
              <div style={{ marginTop: 12 }}>
                <Note type="warn" icon="!">
                  Máy vẫn nạp được USB này và vào tới màn hình cài đặt — vấn đề nằm ở phần
                  nội dung bộ cài bên dưới, nên quá trình cài sẽ dừng lại giữa chừng.
                </Note>
              </div>
            )}
            <div className="legend" style={{ marginTop: 14 }}>
              <span><b style={{ background: "var(--ok)" }} />Đạt · {report.passed}</span>
              <span><b style={{ background: "var(--warn)" }} />Cần biết · {report.warned}</span>
              <span><b style={{ background: "var(--danger)" }} />Không đạt · {report.failed}</span>
              <span><b style={{ background: "var(--text-faint)" }} />Không đọc được · {report.skipped}</span>
            </div>
          </Panel>

          {groups.map((g) => (
            <Panel key={g} title={g}>
              {report.checks.filter((c) => c.group === g).map((c) => <CheckRow key={c.id} c={c} />)}
            </Panel>
          ))}
        </>
      )}

      <Panel title="Đối chiếu lại toàn bộ dữ liệu">
        <Note type="info" icon="i">
          Các mục ở trên đọc cấu trúc ổ — đủ để bắt ghi hụt và thiếu file khởi động. Chúng
          <b style={{ display: "inline" }}> không</b> bắt được ổ USB khai khống dung lượng hay
          bộ nhớ flash sắp chết: hai loại đó nhận hết dữ liệu lúc ghi rồi âm thầm vứt đi, nên
          mọi thứ trông vẫn bình thường. Chỉ có đọc ngược lại và so từng byte mới phát hiện ra.
          Việc này mất gần bằng thời gian ghi.
        </Note>

        {isoDiscarded ? (
          <Note type="warn" icon="!" title="Không đối chiếu được vì file ISO đã bị dọn">
            Bạn đã bật "Xoá file ISO sau khi ghi xong" ở bước trước, nên không còn bản gốc
            để so. Các mục kiểm tra cấu trúc ở trên vẫn có giá trị. Muốn dùng chức năng này
            thì tắt tuỳ chọn xoá ở bước Ghi rồi ghi lại.
          </Note>
        ) : (
          <div className="actions">
            <button className="btn btn--sm" onClick={startReadback} disabled={reading || !request}>
              {reading && <span className="spinner" />}
              {readback ? "Đối chiếu lại" : "Đọc lại và đối chiếu"}
            </button>
          </div>
        )}

        {reading && (
          <div style={{ marginTop: 14 }}>
            <Progress
              value={prog?.percent ?? 0}
              left={prog?.message ?? "Đang bắt đầu…"}
              right={pct(prog?.percent ?? 0)}
              file={prog?.detail ?? null}
              busy={(prog?.percent ?? 0) === 0}
            />
          </div>
        )}

        {readback && !reading && (
          <div style={{ marginTop: 14 }}>
            <Note
              type={readback.matched ? "ok" : "danger"}
              icon={readback.matched ? "✓" : "✕"}
              title={readback.matched ? "Dữ liệu trên USB khớp hoàn toàn" : "Dữ liệu trên USB KHÔNG khớp"}
            >
              {readback.message}
            </Note>

            {readback.actual_sha && readback.expected_sha && (
              <div className="grid" style={{ marginTop: 10 }}>
                <div className="stat">
                  <div className="stat__k">Đọc lại từ USB</div>
                  <div className="stat__v mono">{readback.actual_sha}</div>
                  <div className="stat__note">
                    Đã đọc lại {bytes(readback.compared)} từ ổ USB.
                  </div>
                </div>
                <div className="stat">
                  <div className="stat__k">File ảnh gốc</div>
                  <div className="stat__v mono">{readback.expected_sha}</div>
                </div>
              </div>
            )}

            {(readback.mismatched.length > 0 || readback.missing.length > 0) && (
              <div style={{ marginTop: 10 }}>
                <ul className="reasons">
                  {readback.missing.map((x) => (
                    <li key={`m-${x}`} data-t="blk">Thiếu trên USB: {x}</li>
                  ))}
                  {readback.mismatched.map((x) => (
                    <li key={`d-${x}`} data-t="con">Khác nội dung: {x}</li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}
      </Panel>
    </>
  );
}
