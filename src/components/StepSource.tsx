import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errorText, events } from "../lib/api";
import type { IsoInfo, WindowsRelease } from "../types";
import { bytes, duration, pct, shortPath, speed } from "../lib/format";
import { Note, Panel, Progress } from "./ui";

export function StepSource({
  release,
  iso,
  onIso,
}: {
  release: WindowsRelease | null;
  iso: IsoInfo | null;
  onIso: (i: IsoInfo | null) => void;
}) {
  const [busy, setBusy] = useState<null | "pick" | "download" | "hash">(null);
  const [error, setError] = useState<string | null>(null);
  const [dl, setDl] = useState({ percent: 0, speed_bps: 0, eta_secs: 0, downloaded: 0, total: 0 });
  const [hashPct, setHashPct] = useState(0);
  const [hash, setHash] = useState<string | null>(null);

  async function pickFile() {
    setError(null);
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "Ảnh đĩa Windows", extensions: ["iso"] }],
    });
    if (typeof picked !== "string") return;

    setBusy("pick");
    try {
      onIso(await api.inspectIso(picked));
      setHash(null);
    } catch (e) {
      setError(errorText(e));
      onIso(null);
    } finally {
      setBusy(null);
    }
  }

  async function autoDownload() {
    if (!release) return;
    setError(null);
    setBusy("download");
    try {
      const links = await api.fetchDownloadLinks(release.id, "Vietnamese");
      if (links.length === 0) throw new Error("Microsoft không trả về link tải nào.");

      const dest = await openDialog({ directory: true, title: "Chọn thư mục lưu file ISO" });
      if (typeof dest !== "string") { setBusy(null); return; }

      const target = `${dest}\\${release.id}.iso`;
      const un = await events.onDownloadProgress(setDl);
      try {
        await api.downloadIso(links[0].url, target);
        onIso(await api.inspectIso(target));
      } finally {
        un();
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  }

  async function verify() {
    if (!iso) return;
    setBusy("hash");
    setHashPct(0);
    try {
      const un = await events.onHashProgress(setHashPct);
      try {
        setHash(await api.hashIso(iso.path));
      } finally {
        un();
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  }

  async function openOfficial() {
    if (!release) return;
    await openUrl(await api.officialDownloadPage(release.id));
  }

  return (
    <>
      <div className="main__head">
        <h1>Nguồn bộ cài</h1>
        <p>
          {release
            ? <>Cần một file ISO của <b style={{ display: "inline" }}>{release.name}</b>. Bạn có thể chọn file có sẵn hoặc tải mới.</>
            : "Hãy quay lại bước gợi ý để chọn phiên bản Windows trước."}
        </p>
      </div>

      {release?.source === "volume_license" && (
        <Note type="warn" icon="◈" title="Bản này chỉ phát hành qua kênh doanh nghiệp">
          {release.name} không có trên trang tải công khai của Microsoft. Bạn cần lấy ISO từ
          Microsoft 365 admin center, Volume Licensing Service Center, hoặc Visual Studio
          Subscriptions, rồi chọn file ở đây.
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

          <button className="opt" onClick={autoDownload}
                  disabled={busy !== null || !release || release.source === "volume_license"}>
            <span className="opt__radio" />
            <span>
              <span className="opt__title">Tải tự động từ Microsoft</span>
              <span className="opt__desc">
                Lấy link chính thức rồi tải về. Microsoft đôi khi chặn tải tự động theo khu vực —
                nếu thất bại hãy dùng cách thủ công bên dưới.
              </span>
            </span>
          </button>

          <button className="opt" onClick={openOfficial} disabled={busy !== null || !release}>
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
          </div>

          {!iso.bootable_uefi && (
            <div style={{ marginTop: 12 }}>
              <Note type="warn" icon="!">
                File ISO này không có thư mục EFI nên chỉ khởi động được ở chế độ BIOS cũ.
                Hãy chọn kiểu phân vùng MBR ở bước ghi.
              </Note>
            </div>
          )}

          <div className="actions">
            <button className="btn btn--sm" onClick={verify} disabled={busy !== null}>
              {busy === "hash" && <span className="spinner" />} Tính mã SHA-256
            </button>
            {busy === "hash" && <span style={{ fontSize: 12, color: "var(--text-dim)" }}>{pct(hashPct)}</span>}
          </div>

          {hash && (
            <div style={{ marginTop: 10 }}>
              <div className="stat">
                <div className="stat__k">SHA-256</div>
                <div className="stat__v mono">{hash}</div>
                <div className="stat__note">
                  Đối chiếu với giá trị Microsoft công bố trên trang tải để chắc chắn file không hỏng.
                </div>
              </div>
            </div>
          )}
        </Panel>
      )}
    </>
  );
}
