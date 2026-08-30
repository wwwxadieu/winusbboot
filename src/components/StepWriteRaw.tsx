import { useEffect, useRef, useState } from "react";
import { api, errorText, events } from "../lib/api";
import type { DistroRelease, IsoInfo, UsbDisk, WriteProgress } from "../types";
import { bytes, pct, rateLine } from "../lib/format";
import { Fold, Note, Panel, Progress, Why } from "./ui";

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
  onDiscarded,
}: {
  disk: UsbDisk | null;
  iso: IsoInfo | null;
  release: DistroRelease | null;
  admin: boolean;
  onAdminRelaunch: () => void;
  /** Bước Kiểm tra chỉ mở ra khi ghi xong, nên trạng thái này phải nằm ở App. */
  onDone: (v: boolean) => void;
  /** Báo lên App rằng file ISO đã bị dọn, để bước Kiểm tra biết mà giải thích. */
  onDiscarded: () => void;
}) {
  const [confirmed, setConfirmed] = useState(false);
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [prog, setProg] = useState<WriteProgress | null>(null);
  // Nút bắt đầu ghi nằm cuối trang, còn thanh tiến trình nằm trên đầu — bấm
  // xong mà không cuộn lên thì người dùng nhìn vào một trang không có gì thay
  // đổi và tưởng chưa chạy. Cuộn giúp họ đúng một lần, lúc bắt đầu.
  const progRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (running) progRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [running]);
  // Chỉ dọn được file do ứng dụng tự tải; file người dùng tự chọn là của họ.
  const [cleanup, setCleanup] = useState(true);
  const [discarded, setDiscarded] = useState<string | null>(null);

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

        if (cleanup && iso.managed) {
          try {
            await api.discardIso(iso.path);
            setDiscarded(iso.path.split(/[\\/]/).pop() ?? iso.path);
            onDiscarded();
          } catch (e) {
            // Dọn dẹp hỏng không làm hỏng chiếc USB vừa ghi, nên chỉ ghi chú
            // lại chứ không biến cả bước thành thất bại.
            setDiscarded(null);
            console.warn("không dọn được file ISO:", errorText(e));
          }
        }
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
          Cần chọn xong cả ổ USB lẫn file ISO. Hãy quay lại các bước trước.
        </Note>
      </>
    );
  }

  return (
    <>
      <div className="main__head">
        <h1>Ghi ra USB</h1>
      </div>
      <div ref={progRef} />
      {(running || prog) && !done && (
        <Panel title={`Bước ${prog?.stage_index ?? 1}/${prog?.total_stages ?? 3} · ${STAGE_NAME[prog?.stage ?? "check"] ?? ""}`}>
          <Progress
            value={prog?.percent ?? 0}
            left={prog?.message ?? "Đang bắt đầu…"}
            right={prog ? rateLine(prog) : pct(0)}
            file={prog?.detail ?? null}
            busy={running && (prog?.percent ?? 0) === 0}
          />
        </Panel>
      )}

      {!admin && (
        <Note type="warn" icon="🔑">
          Ghi thẳng ra thiết bị đòi hỏi quyền Administrator.
          <div className="actions">
            <button className="btn btn--sm btn--primary" onClick={onAdminRelaunch}>
              Mở lại với quyền quản trị
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

      {iso.managed && (
        <Fold title="Xoá file ISO sau khi ghi xong" hint={cleanup ? `Có · giải phóng ${bytes(iso.size)}` : "Không"}>
          <label style={{ display: "flex", gap: 9, alignItems: "center", cursor: "pointer" }}>
            <input type="checkbox" checked={cleanup} disabled={running}
                   onChange={(e) => setCleanup(e.target.checked)}
                   style={{ width: 16, height: 16, accentColor: "var(--accent)" }} />
            <span style={{ fontSize: 13 }}>Xoá file ISO ứng dụng đã tải, sau khi ghi xong</span>
          </label>
          <Why>
            Giữ lại thì ghi thêm chiếc USB nữa không phải tải lại, và bước cuối mới đối chiếu
            được từng byte trên USB với bản gốc — việc đó cần chính file này.
          </Why>
        </Fold>
      )}

      {release?.secure_boot === "unsigned" && (
        <Note type="warn" icon="!" title="Nhớ tắt Secure Boot trước khi khởi động">
          {release.name} không có shim ký sẵn — máy đang bật Secure Boot sẽ lặng lẽ bỏ qua USB này.
        </Note>
      )}

      <Note type="danger" icon="⚠">
        Ổ <b style={{ display: "inline" }}>{disk.model}</b> ({bytes(disk.size, 0)}) sẽ bị xoá
        toàn bộ, kể cả bảng phân vùng, và không khôi phục được.
        <Why label="Ghi nguyên khối là gì?">
          Ứng dụng đổ thẳng từng byte của file ISO ra ổ từ byte đầu tiên, tương đương lệnh
          <code> dd</code> trên Linux — vì mã khởi động của ISO Linux nằm ngay trong ảnh đĩa.
          Vì vậy luồng này không có bước Format riêng: thao tác ghi đã xoá và dựng lại toàn bộ
          ổ. Mất khoảng 3–15 phút tuỳ tốc độ USB.
        </Why>
        <label style={{ display: "flex", gap: 9, alignItems: "center", marginTop: 11, cursor: "pointer" }}>
          <input type="checkbox" checked={confirmed} disabled={running}
                 onChange={(e) => setConfirmed(e.target.checked)}
                 style={{ width: 16, height: 16, accentColor: "var(--danger)" }} />
          <span>Tôi đã sao lưu dữ liệu và xác nhận ghi đè đúng ổ này.</span>
        </label>
      </Note>

      {error && <Note type="danger" icon="✕" title="Ghi USB thất bại">{error}</Note>}

      {discarded && (
        <Note type="info" icon="🧹" title="Đã dọn file ISO">
          Đã xoá <b style={{ display: "inline" }}>{discarded}</b> để không chiếm dung lượng ổ đĩa.
        </Note>
      )}

      {done && (
        <Note type="ok" icon="✓" title="USB đã sẵn sàng">
          Cắm vào máy cần cài, vào menu boot (thường là F12, F9 hoặc Esc) rồi chọn thiết bị USB.
          Cắm lại vào Windows mà thấy đòi "format ổ đĩa" thì{" "}
          <b style={{ display: "inline" }}>đừng bấm format</b> — Windows chỉ không đọc được
          phân vùng Linux thôi.
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
