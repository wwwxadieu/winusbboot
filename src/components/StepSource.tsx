import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errorText, events } from "../lib/api";
import type { IsoInfo, OsFamily } from "../types";
import { bytes, duration, pct, shortPath, speed } from "../lib/format";
import { Note, Panel, Progress } from "./ui";

/**
 * Mọi thứ bước này cần biết về bản đã chọn, bất kể là Windows hay Linux.
 *
 * Bọc lại thành một hình dạng chung thay vì nhận thẳng `WindowsRelease` hoặc
 * `DistroRelease`: phần việc ở đây — chọn file, tải có tiến trình, đọc nội dung
 * ISO, tính mã băm — giống hệt nhau ở cả hai họ. Chỉ cách *lấy ra link tải* là
 * khác, nên đúng một hàm `resolve` đủ diễn tả khác biệt đó.
 */
export interface SourcePlan {
  name: string;
  officialPage: () => Promise<string>;
  /** `null` nghĩa là bản này chỉ tải thủ công được. */
  resolve:
    | null
    | (() => Promise<{ url: string; filename: string; sha256: string | null }>);
  /** Vì sao chỉ tải thủ công được. */
  manualNote: string | null;
}

type Verify =
  | { state: "idle" }
  | { state: "running"; percent: number }
  | { state: "done"; hash: string; expected: string | null };

export function StepSource({
  family,
  plan,
  iso,
  onIso,
}: {
  family: OsFamily;
  plan: SourcePlan | null;
  iso: IsoInfo | null;
  onIso: (i: IsoInfo | null) => void;
}) {
  const [busy, setBusy] = useState<null | "pick" | "download">(null);
  const [error, setError] = useState<string | null>(null);
  const [dl, setDl] = useState({ percent: 0, speed_bps: 0, eta_secs: 0, downloaded: 0, total: 0 });
  const [verify, setVerify] = useState<Verify>({ state: "idle" });
  // Mã băm chính thức lấy được lúc tải; giữ lại để đối chiếu sau khi tải xong.
  const [expected, setExpected] = useState<string | null>(null);

  async function pickFile() {
    setError(null);
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "Ảnh đĩa", extensions: ["iso"] }],
    });
    if (typeof picked !== "string") return;

    setBusy("pick");
    try {
      onIso(await api.inspectIso(picked));
      // File tự chọn thì không có mã băm chính thức nào để đối chiếu.
      setExpected(null);
      setVerify({ state: "idle" });
    } catch (e) {
      setError(errorText(e));
      onIso(null);
    } finally {
      setBusy(null);
    }
  }

  async function autoDownload() {
    if (!plan?.resolve) return;
    setError(null);
    setBusy("download");
    try {
      const target = await plan.resolve();

      // Không bắt chọn thư mục nữa: ứng dụng tự tải vào thư mục riêng của nó,
      // và chính vì file nằm ở đó nên bước ghi mới dọn dẹp được nó sau này.
      const dir = await api.isoDownloadDir();
      const path = `${dir}\\${target.filename}`;
      const un = await events.onDownloadProgress(setDl);
      try {
        await api.downloadIso(target.url, path);
        onIso(await api.inspectIso(path));
        setExpected(target.sha256);
        setVerify({ state: "idle" });
      } finally {
        un();
      }

      // Có mã băm chính thức thì đối chiếu luôn, không đợi người dùng bấm: file
      // ISO tải dở hoặc hỏng giữa chừng sẽ ghi ra một chiếc USB không boot được,
      // và lúc đó rất khó đoán nguyên nhân nằm ở đâu.
      if (target.sha256) await runVerify(path, target.sha256);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  }

  async function runVerify(path: string, expect: string | null) {
    setVerify({ state: "running", percent: 0 });
    try {
      const un = await events.onHashProgress((p) => setVerify({ state: "running", percent: p }));
      try {
        const hash = await api.hashIso(path);
        setVerify({ state: "done", hash, expected: expect });
      } finally {
        un();
      }
    } catch (e) {
      setError(errorText(e));
      setVerify({ state: "idle" });
    }
  }

  async function openOfficial() {
    if (!plan) return;
    await openUrl(await plan.officialPage());
  }

  const matched =
    verify.state === "done" && verify.expected
      ? verify.hash.toLowerCase() === verify.expected.toLowerCase()
      : null;

  return (
    <>
      <div className="main__head">
        <h1>Nguồn bộ cài</h1>
        <p>
          {plan
            ? <>Cần một file ISO của <b style={{ display: "inline" }}>{plan.name}</b>. Bạn có thể chọn file có sẵn hoặc tải mới.</>
            : "Hãy quay lại bước trước để chọn phiên bản."}
        </p>
      </div>

      {plan?.manualNote && (
        <Note type="warn" icon="◈" title="Bản này phải tải thủ công">
          {plan.manualNote}
        </Note>
      )}

      {error && <Note type="danger" icon="✕" title="Không hoàn tất được">{error}</Note>}

      <Panel title="Chọn nguồn">
        <div className="grid">
          <button className="opt" onClick={pickFile} disabled={busy !== null}>
            <span className="opt__radio" />
            <span>
              <span className="opt__title">Dùng file ISO có sẵn trên máy</span>
              <span className="opt__desc">Cách chắc chắn nhất — chọn file .iso bạn đã tải về từ trước.</span>
            </span>
          </button>

          {/* Chỉ dựng nút này khi thật sự có đường tải tự động. Một nút mờ đi
              vẫn là một lời hứa: người dùng sẽ đi tìm cách bật nó lên. Bản nào
              chỉ tải thủ công được thì khung cảnh báo phía trên đã nói rõ vì
              sao, và ở đây chỉ còn hai lựa chọn dùng được thật. */}
          {plan?.resolve && (
            <button className="opt" onClick={autoDownload} disabled={busy !== null}>
              <span className="opt__radio" />
              <span>
                <span className="opt__title">Tải tự động từ nguồn chính thức</span>
                <span className="opt__desc">
                  Tra tên file hiện hành trong danh sách mã băm chính thức, tải về rồi tự
                  đối chiếu mã băm. File lưu vào thư mục riêng của ứng dụng.
                </span>
              </span>
            </button>
          )}

          <button className="opt" onClick={openOfficial} disabled={busy !== null || !plan}>
            <span className="opt__radio" />
            <span>
              <span className="opt__title">Mở trang tải chính thức trong trình duyệt</span>
              <span className="opt__desc">Tải thủ công rồi quay lại đây chọn file.</span>
            </span>
          </button>
        </div>

        {busy === "download" && (
          <div style={{ marginTop: 16 }}>
            <Progress
              value={dl.percent}
              left={`Đang tải · ${bytes(dl.downloaded)} / ${bytes(dl.total)}`}
              right={`${speed(dl.speed_bps)} · còn ${duration(dl.eta_secs)}`}
            />
          </div>
        )}
        {busy === "pick" && (
          <div style={{ marginTop: 16 }}>
            <Progress value={100} busy left="Đang đọc nội dung file ISO…" right="" />
          </div>
        )}
      </Panel>

      {iso && (
        <Panel title="File ISO đã chọn">
          <div className="grid grid--2">
            <div className="stat">
              <div className="stat__k">Đường dẫn</div>
              <div className="stat__v" style={{ fontSize: 13 }}>{shortPath(iso.path, 60)}</div>
              <div className="stat__note">{bytes(iso.size)}</div>
            </div>

            {family === "windows" ? (
              <>
                <div className="stat">
                  <div className="stat__k">Ảnh cài đặt</div>
                  <div className="stat__v" style={{ fontSize: 13 }}>{iso.install_image ?? "Không tìm thấy"}</div>
                  <div className="stat__note">
                    {bytes(iso.install_image_size)}
                    {iso.needs_split && " · lớn hơn 4 GB nên sẽ được tách tự động"}
                  </div>
                </div>
                <div className="stat">
                  <div className="stat__k">Kiến trúc</div>
                  <div className="stat__v">{iso.architecture}</div>
                  <div className="stat__note">{iso.bootable_uefi ? "Khởi động được UEFI" : "Không thấy thư mục EFI"}</div>
                </div>
                <div className="stat">
                  <div className="stat__k">Phiên bản bên trong</div>
                  <div className="stat__v" style={{ fontSize: 13 }}>
                    {iso.editions.length ? iso.editions.slice(0, 3).join(", ") : "Không đọc được"}
                  </div>
                  {iso.editions.length > 3 && <div className="stat__note">và {iso.editions.length - 3} bản khác</div>}
                </div>
              </>
            ) : (
              <div className="stat">
                <div className="stat__k">Khởi động</div>
                <div className="stat__v">{iso.bootable_uefi ? "UEFI + BIOS" : "BIOS cũ"}</div>
                <div className="stat__note">
                  {iso.bootable_uefi
                    ? "Có thư mục EFI — boot được cả máy đời mới lẫn đời cũ."
                    : "Không thấy thư mục EFI; chỉ boot được ở chế độ BIOS/CSM."}
                </div>
              </div>
            )}
          </div>

          {family === "windows" && !iso.bootable_uefi && (
            <div style={{ marginTop: 12 }}>
              <Note type="warn" icon="!">
                File ISO này không có thư mục EFI nên chỉ khởi động được ở chế độ BIOS cũ.
                Hãy chọn kiểu phân vùng MBR ở bước ghi.
              </Note>
            </div>
          )}

          <div className="actions">
            <button className="btn btn--sm" onClick={() => runVerify(iso.path, expected)}
                    disabled={busy !== null || verify.state === "running"}>
              {verify.state === "running" && <span className="spinner" />}
              {expected ? "Kiểm tra lại mã băm" : "Tính mã SHA-256"}
            </button>
            {verify.state === "running" && (
              <span style={{ fontSize: 12, color: "var(--text-dim)" }}>{pct(verify.percent)}</span>
            )}
          </div>

          {verify.state === "done" && (
            <div style={{ marginTop: 10 }}>
              {matched === true && (
                <Note type="ok" icon="✓" title="Mã băm khớp với công bố chính thức">
                  File tải về nguyên vẹn, đúng bản do dự án phát hành.
                </Note>
              )}
              {matched === false && (
                <Note type="danger" icon="✕" title="Mã băm KHÔNG khớp">
                  File này khác với bản dự án công bố — có thể tải lỗi hoặc đã bị sửa đổi.
                  Đừng ghi ra USB; hãy xoá file và tải lại.
                </Note>
              )}
              <div className="stat" style={{ marginTop: matched === null ? 0 : 10 }}>
                <div className="stat__k">SHA-256 của file</div>
                <div className="stat__v mono">{verify.hash}</div>
                {verify.expected ? (
                  <div className="stat__note mono">Công bố chính thức: {verify.expected}</div>
                ) : (
                  <div className="stat__note">
                    Đối chiếu với giá trị nhà phát hành công bố trên trang tải để chắc chắn file không hỏng.
                  </div>
                )}
              </div>
            </div>
          )}
        </Panel>
      )}
    </>
  );
}
