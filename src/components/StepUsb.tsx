import type { UsbDisk } from "../types";
import { bytes } from "../lib/format";
import { Empty, Note, Panel, UsbIcon } from "./ui";

function reasonUnusable(d: UsbDisk): string | null {
  if (d.is_system || d.is_boot) return "Ổ hệ thống — không thể dùng";
  if (d.is_readonly) return "Ổ đang ở chế độ chỉ đọc";
  if (d.size < 8 * 1024 ** 3) return "Dưới 8 GB, không đủ chứa bộ cài";
  return null;
}

export function StepUsb({
  disks,
  selected,
  onSelect,
  onRefresh,
  loading,
}: {
  disks: UsbDisk[];
  selected: number | null;
  onSelect: (n: number) => void;
  onRefresh: () => void;
  loading: boolean;
}) {
  return (
    <>
      <div className="main__head">
        <h1>Chọn ổ USB</h1>
        <p>
          Ứng dụng theo dõi liên tục các ổ USB đang cắm — cắm hoặc rút ổ thì danh sách
          dưới đây tự cập nhật sau vài giây, không cần bấm gì.
        </p>
      </div>

      <Note type="danger" icon="⚠">
        Toàn bộ dữ liệu trên ổ USB được chọn <b style={{ display: "inline" }}>sẽ bị xoá sạch</b> ở
        bước cuối. Hãy sao lưu trước nếu ổ đang chứa dữ liệu quan trọng.
      </Note>

      <Panel title={`Ổ USB phát hiện được (${disks.length})`}>
        {disks.length === 0 ? (
          <Empty icon="🔌" title={loading ? "Đang dò tìm…" : "Chưa thấy ổ USB nào"}>
            {loading ? "Đang đọc danh sách thiết bị." : "Hãy cắm ổ USB vào máy — danh sách sẽ tự hiện ra."}
          </Empty>
        ) : (
          <div className="grid">
            {disks.map((d) => {
              const blocked = reasonUnusable(d);
              const letters = d.volumes.map((v) => v.letter).filter(Boolean).join(", ");
              return (
                <button
                  key={d.number}
                  className="disk"
                  aria-pressed={selected === d.number}
                  disabled={blocked !== null}
                  onClick={() => onSelect(d.number)}
                >
                  <span className="disk__icon"><UsbIcon /></span>
                  <span style={{ minWidth: 0 }}>
                    <span className="disk__name">{d.model}</span>
                    <span className="disk__meta">
                      {blocked ?? (
                        <>
                          Ổ đĩa {d.number}
                          {letters && ` · ${letters}:`}
                          {d.volumes[0]?.fs && ` · ${d.volumes[0].fs}`}
                          {d.volumes[0]?.label && ` · "${d.volumes[0].label}"`}
                        </>
                      )}
                    </span>
                  </span>
                  <span className="disk__size">
                    <b>{bytes(d.size, 0)}</b>
                    <span>{d.partition_style}</span>
                  </span>
                </button>
              );
            })}
          </div>
        )}

        <div className="actions">
          <button className="btn btn--sm" onClick={onRefresh} disabled={loading}>
            {loading && <span className="spinner" />} Quét lại
          </button>
          <span style={{ fontSize: 12, color: "var(--text-faint)" }}>
            Chỉ ổ cắm qua cổng USB mới hiện ở đây; ổ cứng trong máy được lọc bỏ hoàn toàn.
          </span>
        </div>
      </Panel>
    </>
  );
}
