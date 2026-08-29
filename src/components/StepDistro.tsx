import type { DesktopWeight, DistroCandidate, DistroRecommendation, Verdict } from "../types";
import { bytes } from "../lib/format";
import { Empty, Note, Panel } from "./ui";

const VERDICT_LABEL: Record<Verdict, string> = {
  recommended: "Cài được ngay",
  needs_setup: "Cần tắt Secure Boot",
  needs_bypass: "Chạy được nhưng ì",
  blocked: "Không cài được",
};

const WEIGHT_LABEL: Record<DesktopWeight, string> = {
  light: "Desktop nhẹ",
  medium: "Desktop vừa",
  heavy: "Desktop nặng",
};

/** Bản rolling không có mốc hết hạn nên không tô màu cảnh báo vòng đời. */
function lifecycleTone(c: DistroCandidate): Verdict | undefined {
  if (c.release.rolling) return undefined;
  if (c.expired) return "blocked";
  return undefined;
}

function DistroCard({
  c,
  selected,
  onSelect,
}: {
  c: DistroCandidate;
  selected: boolean;
  onSelect: () => void;
}) {
  // Chỉ nêu lý do quan trọng nhất, giống thẻ phiên bản Windows: một dòng vướng
  // mắc, hoặc một điểm cộng. Liệt kê hết thì thẻ dài gấp ba mà vẫn chỉ đọc dòng đầu.
  const headline = c.blockers[0] ?? c.cons[0] ?? c.pros[0] ?? c.release.tagline;
  const tone = c.blockers.length ? "blk" : c.cons.length ? "con" : "pro";

  return (
    <button className="rec" aria-pressed={selected} onClick={onSelect}>
      <div className="rec__top">
        <span className="rec__name">{c.release.name}</span>
        <span className="rec__score">{c.score}</span>
      </div>

      <div className="rec__tags">
        <span className="pill" data-v={c.verdict}>{VERDICT_LABEL[c.verdict]}</span>
        <span className="pill" data-v={lifecycleTone(c)}>{c.support_label}</span>
        {c.release.lts && <span className="pill">LTS</span>}
        <span className="rec__build">{c.release.desktop}</span>
      </div>

      <div className={`rec__line rec__line--${tone}`}>
        <b>{tone === "blk" ? "✕" : tone === "con" ? "!" : "✓"}</b>
        {headline}
      </div>

      <div className="rec__who">Hợp với: {c.release.audience}</div>
    </button>
  );
}

export function StepDistro({
  rec,
  loading,
  chosen,
  onChoose,
  onSeeHardware,
}: {
  rec: DistroRecommendation | null;
  loading: boolean;
  chosen: string | null;
  onChoose: (id: string) => void;
  onSeeHardware: () => void;
}) {
  if (!rec) {
    return (
      <>
        <div className="main__head"><h1>Chọn bản Linux</h1></div>
        <Panel>
          <Empty icon="🐧" title={loading ? "Đang phân tích…" : "Chưa có kết quả"}>
            {loading
              ? "Đang đối chiếu cấu hình máy với yêu cầu của từng bản phân phối."
              : "Hãy quét lại phần cứng."}
          </Empty>
        </Panel>
      </>
    );
  }

  const top = rec.candidates.find((c) => c.verdict !== "blocked");
  const noteType = !top ? "danger" : top.verdict === "recommended" ? "ok" : "warn";
  const selected = rec.candidates.find((c) => c.release.id === (chosen ?? rec.best));

  return (
    <>
      <div className="main__head">
        <h1>Chọn bản Linux</h1>
        <p>
          Điểm số phản ánh mức độ hợp giữa cấu hình máy và từng bản — nặng nhất là lượng RAM
          so với môi trường desktop, vì đó mới là thứ quyết định máy chạy mượt hay ì.
        </p>
      </div>

      <Note type={noteType} icon="◈" title="Kết luận">
        {rec.summary}
        <div className="actions">
          <button className="btn btn--sm btn--ghost" onClick={onSeeHardware}>
            Máy có {rec.ram_gb.toFixed(1)} GB RAM · {rec.architecture} — xem chi tiết phần cứng →
          </button>
        </div>
      </Note>

      <Panel title="Các bản phân phối, xếp theo điểm">
        <div className="grid grid--2">
          {rec.candidates.map((c) => (
            <DistroCard
              key={c.release.id}
              c={c}
              selected={(chosen ?? rec.best) === c.release.id}
              onSelect={() => onChoose(c.release.id)}
            />
          ))}
        </div>
      </Panel>

      {selected && (
        <Panel title={`Yêu cầu của ${selected.release.name}`}>
          <div className="grid grid--3">
            <div className="stat">
              <div className="stat__k">RAM</div>
              <div className="stat__v">{selected.release.rec_ram_gb} GB</div>
              <div className="stat__note">
                khuyến nghị · tối thiểu {selected.release.min_ram_gb} GB
              </div>
            </div>
            <div className="stat">
              <div className="stat__k">Dung lượng ổ</div>
              <div className="stat__v">{selected.release.min_disk_gb} GB</div>
              <div className="stat__note">tối thiểu để cài</div>
            </div>
            <div className="stat">
              <div className="stat__k">File ISO</div>
              <div className="stat__v">~{bytes(selected.release.iso_size, 0)}</div>
              <div className="stat__note">{WEIGHT_LABEL[selected.release.weight]}</div>
            </div>
          </div>

          {selected.release.secure_boot === "unsigned" && (
            <div style={{ marginTop: 12 }}>
              <Note type="warn" icon="!" title="Bản này không có shim ký sẵn">
                Máy đang bật Secure Boot sẽ không boot được USB này, và triệu chứng là máy
                lặng lẽ bỏ qua USB chứ không báo lỗi gì. Vào BIOS tắt Secure Boot trước khi
                khởi động từ USB.
              </Note>
            </div>
          )}

          {selected.cons.length > 0 && (
            <div style={{ marginTop: 12 }}>
              <ul className="reasons">
                {selected.cons.map((x) => <li key={x} data-t="con">{x}</li>)}
                {selected.blockers.map((x) => <li key={x} data-t="blk">{x}</li>)}
              </ul>
            </div>
          )}
        </Panel>
      )}

      <Panel title="Nguồn dữ liệu">
        <Note type="info" icon="i">
          Bảng phiên bản trong ứng dụng chốt ngày{" "}
          {rec.catalog_snapshot.split("-").reverse().join("/")} và không tự đồng bộ — các dự án
          Linux ra bản mới theo nhịp riêng. Link tải thì luôn đúng: ứng dụng tra tên file ISO
          hiện hành từ file mã băm chính thức ngay lúc bạn bấm tải, nên bản vá nhỏ mới ra vẫn
          tải đúng file mới.
        </Note>
      </Panel>
    </>
  );
}
