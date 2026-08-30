import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, errorText, events } from "../lib/api";
import type { DeviceMatch, DriverAnalysis, DriverFilter, StageReport } from "../types";
import { bytes, shortPath } from "../lib/format";
import { Note, Panel, Progress } from "./ui";

/**
 * Bước kèm driver vào USB.
 *
 * Lý do bước này tồn tại: cài lại Windows xong thì máy hay mất Wi-Fi, mà muốn
 * tải driver Wi-Fi thì lại cần Wi-Fi. Đưa driver lên USB từ trước là cách duy
 * nhất phá được vòng luẩn quẩn đó.
 */

const FILTERS: { id: DriverFilter; title: string; desc: string }[] = [
  {
    id: "essential",
    title: "Chỉ mạng và ổ đĩa",
    desc: "Wi-Fi, Bluetooth, mạng dây, driver điều khiển ổ đĩa. Ít rủi ro nhất, nhẹ nhất.",
  },
  {
    id: "recommended",
    title: "Khuyến nghị",
    desc: "Thêm chipset, USB, bàn phím, chuột, âm thanh, màn hình. Bỏ card đồ hoạ — nhóm này hay gây lỗi khi nhồi sẵn, mà thiếu thì Windows vẫn chạy rồi tự cập nhật.",
  },
  {
    id: "all",
    title: "Tất cả",
    desc: "Chép mọi gói tìm được, kể cả card đồ hoạ và các phần mềm phụ trợ của hãng.",
  },
];

const KIND_LABEL: Record<DeviceMatch["kind"], string> = {
  wifi: "Wi-Fi",
  ethernet: "Mạng dây",
  bluetooth: "Bluetooth",
  storage: "Ổ đĩa",
};

/** Thiết bị thiếu driver mà thiếu là đau nhất — sắp lên đầu danh sách. */
function severity(d: DeviceMatch): number {
  if (d.covered_by) return 3;
  return d.kind === "wifi" ? 0 : d.kind === "storage" ? 1 : 2;
}

export function StepDrivers({
  driveLetter,
  admin,
  onAdminRelaunch,
  onStaged,
}: {
  driveLetter: string | null;
  admin: boolean;
  onAdminRelaunch: () => void;
  onStaged: (r: StageReport | null) => void;
}) {
  const [source, setSource] = useState<string | null>(null);
  const [exportDir, setExportDir] = useState<string | null>(null);
  const [filter, setFilter] = useState<DriverFilter>("recommended");

  const [busy, setBusy] = useState<null | "export" | "scan" | "copy">(null);
  const [exportAt, setExportAt] = useState({ done: 0, total: 0 });
  const [copyAt, setCopyAt] = useState({ done: 0, total: 0, name: "" });

  const [analysis, setAnalysis] = useState<DriverAnalysis | null>(null);
  const [report, setReport] = useState<StageReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [cleaned, setCleaned] = useState(false);

  useEffect(() => {
    api.driverExportDir().then(setExportDir).catch(() => setExportDir(null));
  }, []);

  // Quét lại mỗi khi đổi nguồn hoặc đổi mức lọc: phần đối chiếu với thiết bị
  // thật phải phản ánh đúng bộ sắp chép, chứ không phải cả thư mục trên đĩa.
  const analyse = useCallback(async (path: string, f: DriverFilter) => {
    setBusy("scan");
    setError(null);
    try {
      setAnalysis(await api.analyseDrivers(path, f));
    } catch (e) {
      setError(errorText(e));
      setAnalysis(null);
    } finally {
      setBusy(null);
    }
  }, []);

  useEffect(() => {
    if (source) void analyse(source, filter);
  }, [source, filter, analyse]);

  async function pickFolder() {
    setError(null);
    const picked = await openDialog({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    setReport(null);
    onStaged(null);
    setSource(picked);
  }

  async function exportFromThisPc() {
    setError(null);
    setBusy("export");
    setExportAt({ done: 0, total: 0 });
    const un = await events.onDriverExport((done, total) => setExportAt({ done, total }));
    try {
      const dir = await api.exportSystemDrivers();
      setReport(null);
      onStaged(null);
      setCleaned(false);
      setSource(dir);
    } catch (e) {
      setError(errorText(e));
    } finally {
      un();
      setBusy(null);
    }
  }

  async function copyToUsb() {
    if (!source || !driveLetter) return;
    setError(null);
    setBusy("copy");
    setCopyAt({ done: 0, total: 0, name: "" });
    const un = await events.onDriverCopy((done, total, name) => setCopyAt({ done, total, name }));
    try {
      const r = await api.stageDrivers(source, filter, driveLetter);
      setReport(r);
      onStaged(r);
    } catch (e) {
      setError(errorText(e));
    } finally {
      un();
      setBusy(null);
    }
  }

  async function cleanExport() {
    try {
      await api.discardDriverExport();
      setCleaned(true);
    } catch (e) {
      setError(errorText(e));
    }
  }

  const devices = analysis ? [...analysis.devices].sort((a, b) => severity(a) - severity(b)) : [];
  const missingWifi = devices.filter((d) => d.kind === "wifi" && !d.covered_by);
  const selected = analysis?.set.packages.filter((p) => analysis.selected.includes(p.name)) ?? [];
  const hasNetwork = selected.some((p) => p.is_network);
  const fromExport = !!(source && exportDir && source.toLowerCase() === exportDir.toLowerCase());

  return (
    <>
      <div className="main__head">
        <h1>Driver kèm theo USB</h1>
        <p>
          Cài lại Windows xong máy hay mất Wi-Fi, mà tải driver Wi-Fi thì lại cần Wi-Fi. Đặt
          driver lên USB từ bây giờ thì Windows Setup tự cài chúng trong lúc cài máy — xong là
          có mạng ngay. Bước này không bắt buộc; bỏ qua vẫn dùng USB bình thường.
        </p>
      </div>

      {!driveLetter && (
        <Note type="warn" icon="!" title="Chưa có ổ để chép">
          Hãy hoàn tất bước ghi bộ cài trước — driver được chép vào chính chiếc USB đó.
        </Note>
      )}

      {error && <Note type="danger" icon="✕" title="Không hoàn tất được">{error}</Note>}

      <Panel title="Lấy driver từ đâu">
        <div className="grid">
          <button className="opt" onClick={exportFromThisPc} disabled={busy !== null || !admin}>
            <span className="opt__radio" />
            <span>
              <span className="opt__title">
                Xuất driver của chính máy này
                {!admin && " — cần quyền quản trị"}
              </span>
              <span className="opt__desc">
                {admin
                  ? "Lấy đúng bộ driver đang chạy được trên máy này. Chính xác nhất nếu bạn đang cài lại cho chính chiếc máy đang dùng. Mất vài phút và khoảng 0,5–3 GB."
                  : "Ứng dụng cần quyền quản trị mới đọc được kho driver của Windows."}
              </span>
            </span>
          </button>

          <button className="opt" onClick={pickFolder} disabled={busy !== null}>
            <span className="opt__radio" />
            <span>
              <span className="opt__title">Chọn thư mục driver có sẵn</span>
              <span className="opt__desc">
                Bộ driver bạn đã tải từ trang của hãng và giải nén ra. Ứng dụng tự tìm mọi file
                .inf bên trong, kể cả nằm sâu nhiều tầng thư mục.
              </span>
            </span>
          </button>
        </div>

        {!admin && (
          <div className="actions">
            <button className="btn btn--sm" onClick={onAdminRelaunch}>
              Chạy lại với quyền quản trị
            </button>
          </div>
        )}

        {busy === "export" && (
          <div style={{ marginTop: 16 }}>
            <Progress
              value={exportAt.total ? (exportAt.done / exportAt.total) * 100 : 0}
              busy={!exportAt.total}
              left="Đang xuất driver của máy…"
              right={exportAt.total ? `${exportAt.done}/${exportAt.total} gói` : "Đang đếm…"}
            />
          </div>
        )}
        {busy === "scan" && (
          <div style={{ marginTop: 16 }}>
            <Progress value={100} busy left="Đang đọc các file .inf…" right="" />
          </div>
        )}

        {source && (
          <div className="stat" style={{ marginTop: 14 }}>
            <div className="stat__k">Thư mục đang dùng</div>
            <div className="stat__v" style={{ fontSize: 13 }}>{shortPath(source, 64)}</div>
            {analysis && (
              <div className="stat__note">
                {analysis.set.packages.length} gói driver · {bytes(analysis.set.total_size)}
              </div>
            )}
          </div>
        )}
      </Panel>

      {analysis && analysis.set.packages.length === 0 && (
        <Note type="warn" icon="◈" title="Không thấy file .inf nào trong thư mục này">
          {analysis.set.installer_only > 0
            ? `Thư mục chỉ có ${analysis.set.installer_only} bộ cài dạng .exe/.msi. Windows Setup chỉ nhồi được driver dạng .inf vào ảnh cài, nên hãy tải bản "driver only" (đôi khi ghi là bản .zip hoặc bản cho IT) thay vì bản cài đặt.`
            : "Hãy kiểm tra lại đường dẫn — có thể driver nằm trong một thư mục con khác."}
        </Note>
      )}

      {analysis && analysis.set.packages.length > 0 && (
        <>
          <Panel title="Thiết bị của máy này">
            {!analysis.devices_read ? (
              <Note type="warn" icon="!">
                Không đọc được danh sách thiết bị của máy, nên không đối chiếu được. Driver vẫn
                chép lên USB bình thường — chỉ là ứng dụng không khẳng định được máy đã đủ hay chưa.
              </Note>
            ) : devices.length === 0 ? (
              <Note type="warn" icon="!">
                Không thấy card mạng hay ổ đĩa nào để đối chiếu.
              </Note>
            ) : (
              <>
                {devices.map((d) => (
                  <div className="check" key={d.hardware_id + d.name}>
                    <span className="check__dot" data-s={d.covered_by ? "pass" : "fail"}>
                      {d.covered_by ? "✓" : "✕"}
                    </span>
                    <div className="check__body">
                      <div className="check__head">
                        <span className="check__label">{d.name}</span>
                        <span className="check__req">{KIND_LABEL[d.kind]}</span>
                      </div>
                      <div className="check__value">
                        {d.covered_by
                          ? `Có driver trong gói ${d.covered_by}`
                          : "Chưa có driver nào trong bộ đã chọn khớp thiết bị này"}
                      </div>
                    </div>
                  </div>
                ))}
                {missingWifi.length > 0 && (
                  <div style={{ marginTop: 12 }}>
                    <Note type="danger" icon="!" title="Card Wi-Fi chưa có driver">
                      Cài xong máy sẽ không có Wi-Fi. Hãy thử mức lọc rộng hơn, hoặc tải bộ driver
                      Wi-Fi dạng .inf từ trang của hãng rồi chọn lại thư mục.
                    </Note>
                  </div>
                )}
              </>
            )}
          </Panel>

          <Panel title="Chép những nhóm nào">
            <div className="grid">
              {FILTERS.map((f) => (
                <button
                  key={f.id}
                  className="opt"
                  aria-pressed={filter === f.id}
                  disabled={busy !== null}
                  onClick={() => setFilter(f.id)}
                >
                  <span className="opt__radio" />
                  <span>
                    <span className="opt__title">{f.title}</span>
                    <span className="opt__desc">{f.desc}</span>
                  </span>
                </button>
              ))}
            </div>

            <div className="grid grid--3" style={{ marginTop: 14 }}>
              <div className="stat">
                <div className="stat__k">Sẽ chép</div>
                <div className="stat__v">{selected.length} gói</div>
                <div className="stat__note">trong tổng số {analysis.set.packages.length}</div>
              </div>
              <div className="stat">
                <div className="stat__k">Dung lượng</div>
                <div className="stat__v">{bytes(analysis.selected_size)}</div>
                <div className="stat__note">chép vào thư mục $WinPEDriver$</div>
              </div>
              <div className="stat">
                <div className="stat__k">Driver mạng</div>
                <div className="stat__v">{hasNetwork ? "Có" : "Không có"}</div>
                <div className="stat__note">
                  {hasNetwork ? "máy sẽ có mạng ngay sau khi cài" : "cài xong sẽ không có mạng"}
                </div>
              </div>
            </div>

            {selected.length > 0 && (
              <div style={{ marginTop: 12 }}>
                <div className="panel__title">Danh sách gói</div>
                <div className="grid grid--2">
                  {selected.slice(0, 12).map((p) => (
                    <div className="stat" key={p.folder}>
                      <div className="stat__k">{p.classes.join(", ") || "không rõ nhóm"}</div>
                      <div className="stat__v" style={{ fontSize: 13 }}>{p.name}</div>
                      <div className="stat__note">
                        {[p.provider, p.version].filter(Boolean).join(" · ") || p.infs[0]}
                        {" · "}
                        {bytes(p.size)}
                      </div>
                    </div>
                  ))}
                </div>
                {selected.length > 12 && (
                  <div className="stat__note" style={{ marginTop: 8 }}>
                    và {selected.length - 12} gói khác
                  </div>
                )}
              </div>
            )}

            {analysis.set.installer_only > 0 && (
              <div style={{ marginTop: 12 }}>
                <Note type="info" icon="i" title="Bỏ qua một số bộ cài dạng .exe">
                  Có {analysis.set.installer_only} thư mục chỉ chứa file cài đặt .exe/.msi. Windows
                  Setup chỉ nhồi được driver dạng .inf vào ảnh cài, nên những thứ đó không chép
                  được — hãy chạy chúng bằng tay sau khi cài xong nếu cần.
                </Note>
              </div>
            )}

            <div className="actions">
              <button
                className="btn btn--primary"
                disabled={busy !== null || !driveLetter || selected.length === 0}
                onClick={copyToUsb}
              >
                {busy === "copy" && <span className="spinner" />}
                Chép driver vào USB
              </button>
            </div>

            {busy === "copy" && (
              <div style={{ marginTop: 14 }}>
                <Progress
                  value={copyAt.total ? (copyAt.done / copyAt.total) * 100 : 0}
                  left={`Đang chép ${copyAt.done}/${copyAt.total} gói`}
                  right=""
                  file={copyAt.name || null}
                />
              </div>
            )}
          </Panel>
        </>
      )}

      {report && (
        <Panel title="Đã chép xong">
          <Note type="ok" icon="✓" title={`Đã đặt ${report.packages} gói driver lên USB`}>
            {bytes(report.bytes)} nằm ở {report.dest}. Windows Setup tự tìm thư mục này và cài
            các driver trong đó vào máy — không cần bạn làm gì thêm sau khi cài.
          </Note>

          {fromExport && (
            <div className="actions">
              <button className="btn btn--sm" onClick={cleanExport} disabled={cleaned}>
                {cleaned ? "Đã xoá bản xuất tạm" : "Xoá bản xuất tạm trên máy"}
              </button>
              <span style={{ fontSize: 12, color: "var(--text-dim)" }}>
                {cleaned
                  ? "Driver trên USB không bị ảnh hưởng."
                  : "Bản xuất nằm trong thư mục riêng của ứng dụng; giữ lại thì lần sau khỏi xuất lại."}
              </span>
            </div>
          )}
        </Panel>
      )}
    </>
  );
}
