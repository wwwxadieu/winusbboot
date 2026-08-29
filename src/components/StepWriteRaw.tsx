import { useEffect, useState } from "react";
import { api, errorText, events } from "../lib/api";
import type { DistroRelease, IsoInfo, UsbDisk, WriteProgress } from "../types";
import { bytes, pct } from "../lib/format";
import { Note, Panel, Progress } from "./ui";

const STAGE_NAME: Record<string, string> = {
  check: "Kiểm tra an toàn",
  raw: "Ghi ảnh đĩa",
  flush: "Đẩy bộ đệm ra USB",
};

/**
 * Bước ghi của luồng Linux.
 *
 * Tách khỏi `StepWrite` của Windows thay vì thêm một cờ chế độ: hai bên khác
 * nhau ở gần như mọi thứ — không có cài đặt tự động, không có kiểu phân vùng,
 * không có tách install.wim — và quan trọng nhất là **ô xác nhận xoá nằm ở
 * đây**. Luồng Windows đã xác nhận ở bước Format riêng, còn luồng Linux ghi
 * thẳng từ byte 0 nên chính bước này là lúc dữ liệu biến mất.
 */
export function StepWriteRaw({
  disk,
  iso,
  release,
  admin,
  onAdminRelaunch,
  onDone,
}: {
  disk: UsbDisk | null;
  iso: IsoInfo | null;
  release: DistroRelease | null;
  admin: boolean;
  onAdminRelaunch: () => void;
  /** Bước Kiểm tra chỉ mở ra khi ghi xong, nên trạng thái này phải nằm ở App. */
  onDone: (v: boolean) => void;
}) {
  const [confirmed, setConfirmed] = useState(false);
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [prog, setProg] = useState<WriteProgress | null>(null);

  // Đổi ổ hay đổi file thì mọi thứ đã xác nhận trước đó không còn giá trị.
  useEffect(() => {
    setConfirmed(false);
    setDone(false);
    onDone(false);
    setError(null);
    setProg(null);
  }, [disk?.number, iso?.path, onDone]);

  async function start() {
    if (!disk || !iso) return;
    setError(null);
    setDone(false);
    setRunning(true);
    try {
      // Lấy vân tay ổ ngay trước khi ghi thay vì dùng giá trị đọc lúc chọn:
      // rút ra cắm ổ khác vào cùng vị trí thì backend sẽ từ chối.
      const token = await api.diskToken(disk.number);
      const un = await events.onWriteProgress(setProg);
      try {
        await api.writeImageRaw({
          disk_number: disk.number,
          iso_path: iso.path,
          confirm_token: token,
        });
        setDone(true);
        onDone(true);
      } finally {
        un();
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setRunning(false);
    }
  }

  if (!disk || !iso) {
    return (
      <>
        <div className="main__head"><h1>Ghi ra USB</h1></div>
        <Note type="warn" icon="!">
          Cần chọn xong cả ổ USB lẫn file ISO thì mới ghi được. Hãy quay lại các bước trước.
        </Note>
      </>
    );
  }

  return (
    <>
      <div className="main__head">
        <h1>Ghi ra USB</h1>
        <p>
          Ghi nguyên khối file ISO ra ổ USB — tương đương lệnh <code>dd</code>. Mất khoảng
          3–15 phút tuỳ tốc độ USB.
        </p>
      </div>

      {!admin && (
        <Note type="warn" icon="🔑" title="Cần quyền quản trị">
          Ghi thẳng ra thiết bị đòi hỏi quyền Administrator.
          <div className="actions">
            <button className="btn btn--sm btn--primary" onClick={onAdminRelaunch}>
              Khởi động lại với quyền quản trị
            </button>
          </div>
        </Note>
      )}

      <Panel title="Sẽ ghi">
        <div className="grid grid--3">
          <div className="stat">
            <div className="stat__k">Ổ đích</div>
            <div className="stat__v">Ổ đĩa {disk.number}</div>
            <div className="stat__note">{disk.model} · {bytes(disk.size, 0)}</div>
          </div>
          <div className="stat">
            <div className="stat__k">Ảnh đĩa</div>
            <div className="stat__v" style={{ fontSize: 13 }}>{iso.path.split(/[\\/]/).pop()}</div>
            <div className="stat__note">{bytes(iso.size)}</div>
          </div>
          <div className="stat">
            <div className="stat__k">Hệ điều hành</div>
            <div className="stat__v" style={{ fontSize: 15 }}>{release?.name ?? "Linux"}</div>
            <div className="stat__note">{release?.desktop ?? ""}</div>
          </div>
        </div>
      </Panel>

      {release?.secure_boot === "unsigned" && (
        <Note type="warn" icon="!" title="Nhớ tắt Secure Boot trước khi khởi động">
          {release.name} không có shim ký sẵn. Máy đang bật Secure Boot sẽ bỏ qua USB này mà
          không báo lỗi gì — dễ tưởng là USB hỏng.
        </Note>
      )}

      <Note type="danger" icon="⚠" title="Xác nhận trước khi ghi đè">
        Ghi nguyên khối bắt đầu từ byte đầu tiên của ổ, nên{" "}
        <b style={{ display: "inline" }}>toàn bộ dữ liệu và bảng phân vùng</b> của ổ{" "}
        <b style={{ display: "inline" }}>{disk.model}</b> (ổ đĩa {disk.number}, {bytes(disk.size, 0)})
        sẽ bị xoá và không khôi phục được. Luồng Linux không có bước Format riêng vì thao tác
        này đã bao gồm việc đó.
        <label style={{ display: "flex", gap: 9, alignItems: "center", marginTop: 11, cursor: "pointer" }}>
          <input type="checkbox" checked={confirmed} disabled={running}
                 onChange={(e) => setConfirmed(e.target.checked)}
                 style={{ width: 16, height: 16, accentColor: "var(--danger)" }} />
          <span>Tôi đã sao lưu dữ liệu và xác nhận ghi đè đúng ổ này.</span>
        </label>
      </Note>

      {(running || prog) && !done && (
        <Panel title={`Bước ${prog?.stage_index ?? 1}/${prog?.total_stages ?? 3} · ${STAGE_NAME[prog?.stage ?? "check"] ?? ""}`}>
          <Progress
            value={prog?.percent ?? 0}
            left={prog?.message ?? "Đang bắt đầu…"}
            right={pct(prog?.percent ?? 0)}
            file={prog?.detail ?? null}
            busy={running && (prog?.percent ?? 0) === 0}
          />
        </Panel>
      )}

      {error && <Note type="danger" icon="✕" title="Ghi USB thất bại">{error}</Note>}

      {done && (
        <Note type="ok" icon="✓" title="USB đã sẵn sàng">
          Cắm USB vào máy cần cài, vào menu boot (thường là F12, F9 hoặc Esc tuỳ hãng) và chọn
          thiết bị USB. Windows có thể hiện thông báo "cần format ổ đĩa" khi bạn cắm lại USB
          này — đó là bình thường, vì Windows không đọc được phân vùng Linux;{" "}
          <b style={{ display: "inline" }}>đừng bấm format</b>.
        </Note>
      )}

      <div className="actions">
        <button className="btn btn--danger" onClick={start}
                disabled={!admin || !confirmed || running}>
          {running && <span className="spinner" />}
          {running ? "Đang ghi…" : done ? "Ghi lại lần nữa" : "Bắt đầu ghi ra USB"}
        </button>
        {!confirmed && admin && !running && (
          <span style={{ fontSize: 12, color: "var(--text-faint)" }}>Hãy tick vào ô xác nhận ở trên.</span>
        )}
      </div>
    </>
  );
}
