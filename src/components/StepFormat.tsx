import { useEffect, useState } from "react";
import { api, errorText, events } from "../lib/api";
import type { FormatResult, IsoInfo, PartitionScheme, UsbDisk, WriteProgress } from "../types";
import { bytes, pct } from "../lib/format";
import { Note, Panel, Progress } from "./ui";

const SCHEMES: { id: PartitionScheme; title: string; desc: string }[] = [
  {
    id: "gpt_fat32",
    title: "GPT + FAT32 — khuyến nghị",
    desc: "Chuẩn cho mọi máy UEFI đời mới. File install.wim quá 4 GB sẽ được tách tự động ở bước sau.",
  },
  {
    id: "mbr_fat32",
    title: "MBR + FAT32 — tương thích rộng",
    desc: "Khởi động được cả máy UEFI ở chế độ CSM lẫn máy BIOS đời cũ.",
  },
  {
    id: "mbr_ntfs",
    title: "MBR + NTFS — cho máy BIOS đời cũ",
    desc: "Không bị giới hạn 4 GB mỗi file, nhưng nhiều máy chỉ UEFI sẽ không nhận ổ này.",
  },
];

export function StepFormat({
  disk,
  iso,
  admin,
  scheme,
  onScheme,
  label,
  onLabel,
  result,
  onResult,
  onAdminRelaunch,
}: {
  disk: UsbDisk | null;
  iso: IsoInfo | null;
  admin: boolean;
  scheme: PartitionScheme;
  onScheme: (s: PartitionScheme) => void;
  label: string;
  onLabel: (v: string) => void;
  result: FormatResult | null;
  onResult: (r: FormatResult | null) => void;
  onAdminRelaunch: () => void;
}) {
  const [confirmed, setConfirmed] = useState(false);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [prog, setProg] = useState<WriteProgress | null>(null);

  // Mỗi lần mở lại bước này đều phải tick xác nhận lại — không được nhớ cái tick
  // của lần trước cho một thao tác xoá dữ liệu.
  //
  // Việc xoá kết quả format khi đổi ổ nằm ở App chứ không phải ở đây: component
  // này được gắn lại mỗi lần người dùng quay về bước Format, và xoá kết quả ở
  // đây sẽ làm mất trạng thái đã format dù ổ không hề đổi.
  useEffect(() => {
    setConfirmed(false);
    setError(null);
    setProg(null);
  }, [disk?.number]);

  // ISO không khởi động được UEFI thì GPT là lựa chọn vô nghĩa.
  useEffect(() => {
    if (iso && !iso.bootable_uefi && scheme === "gpt_fat32") onScheme("mbr_ntfs");
  }, [iso, scheme, onScheme]);

  async function start() {
    if (!disk) return;
    setError(null);
    setRunning(true);
    onResult(null);
    try {
      // Lấy vân tay ổ ngay trước khi xoá thay vì dùng giá trị đọc từ trước:
      // nếu người dùng vừa rút ra cắm lại ổ khác, backend sẽ từ chối.
      const token = await api.diskToken(disk.number);
      const un = await events.onFormatProgress(setProg);
      try {
        onResult(await api.formatUsb({
          disk_number: disk.number,
          scheme,
          label: label.trim() || "WINSETUP",
          confirm_token: token,
        }));
      } finally {
        un();
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setRunning(false);
    }
  }

  if (!disk) {
    return (
      <>
        <div className="main__head"><h1>Format USB</h1></div>
        <Note type="warn" icon="!">Hãy quay lại bước đầu và chọn ổ USB trước.</Note>
      </>
    );
  }

  return (
    <>
      <div className="main__head">
        <h1>Format USB</h1>
        <p>
          Xoá sạch ổ và chia lại phân vùng theo chuẩn khởi động. Đây là thao tác duy nhất
          trong ứng dụng làm mất dữ liệu — nên nó có bước xác nhận riêng.
        </p>
      </div>

      {!admin && (
        <Note type="warn" icon="🔑" title="Cần quyền quản trị">
          Chia lại phân vùng ổ USB đòi hỏi quyền Administrator.
          <div className="actions">
            <button className="btn btn--sm btn--primary" onClick={onAdminRelaunch}>
              Khởi động lại với quyền quản trị
            </button>
          </div>
        </Note>
      )}

      <Panel title="Kiểu phân vùng">
        <div className="grid">
          {SCHEMES.map((s) => (
            <button key={s.id} className="opt" aria-pressed={scheme === s.id}
                    onClick={() => onScheme(s.id)} disabled={running}>
              <span className="opt__radio" />
              <span>
                <span className="opt__title">{s.title}</span>
                <span className="opt__desc">{s.desc}</span>
              </span>
            </button>
          ))}
        </div>

        {iso && !iso.bootable_uefi && (
          <div style={{ marginTop: 12 }}>
            <Note type="warn" icon="!">
              File ISO đã chọn không có thư mục EFI nên chỉ khởi động được ở chế độ BIOS cũ.
            </Note>
          </div>
        )}

        <div className="actions">
          <label style={{ fontSize: 12.5, color: "var(--text-dim)" }}>Tên ổ sau khi format</label>
          <input
            value={label}
            maxLength={11}
            disabled={running}
            onChange={(e) => onLabel(e.target.value.toUpperCase())}
            style={{
              font: "inherit", fontSize: 13, padding: "7px 11px", borderRadius: 9,
              border: "1px solid var(--border-hi)", background: "var(--glass-lo)",
              color: "var(--text)", width: 150, userSelect: "text",
            }}
          />
        </div>
      </Panel>

      <Note type="danger" icon="⚠" title="Xác nhận trước khi xoá">
        Ổ <b style={{ display: "inline" }}>{disk.model}</b> (ổ đĩa {disk.number}, {bytes(disk.size, 0)})
        sẽ bị xoá toàn bộ và không khôi phục được.
        <label style={{ display: "flex", gap: 9, alignItems: "center", marginTop: 11, cursor: "pointer" }}>
          <input type="checkbox" checked={confirmed} disabled={running}
                 onChange={(e) => setConfirmed(e.target.checked)}
                 style={{ width: 16, height: 16, accentColor: "var(--danger)" }} />
          <span>Tôi đã sao lưu dữ liệu và xác nhận xoá đúng ổ này.</span>
        </label>
      </Note>

      {running && (
        <Panel title={`Bước ${prog?.stage_index ?? 1}/${prog?.total_stages ?? 2}`}>
          <Progress
            value={prog?.percent ?? 0}
            left={prog?.message ?? "Đang bắt đầu…"}
            right={pct(prog?.percent ?? 0)}
            busy={(prog?.percent ?? 0) === 0}
          />
        </Panel>
      )}

      {error && <Note type="danger" icon="✕" title="Format thất bại">{error}</Note>}

      {result && !running && (
        <Note type="ok" icon="✓" title="Đã format xong">
          Phân vùng {result.filesystem} theo chuẩn {result.partition_style} nằm ở ổ{" "}
          <b style={{ display: "inline" }}>{result.drive_letter}:</b>
          {result.has_data_partition && (
            <> Phần dung lượng vượt quá 32 GB đã được tạo thành một phân vùng NTFS riêng
            tên DATA để không bỏ phí.</>
          )}
        </Note>
      )}

      <div className="actions">
        <button className="btn btn--danger" onClick={start} disabled={!admin || !confirmed || running}>
          {running && <span className="spinner" />}
          {running ? "Đang format…" : result ? "Format lại" : "Bắt đầu format"}
        </button>
        {!confirmed && admin && (
          <span style={{ fontSize: 12, color: "var(--text-faint)" }}>Hãy tick vào ô xác nhận ở trên.</span>
        )}
      </div>
    </>
  );
}
