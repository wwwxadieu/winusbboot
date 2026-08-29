import type { Check, CheckStatus, CheckSummary, HardwareReport } from "../types";
import { bytes } from "../lib/format";
import { Empty, Note, Panel, Stat } from "./ui";

const MEM_TYPES: Record<number, string> = {
  20: "DDR", 21: "DDR2", 24: "DDR3", 26: "DDR4", 34: "DDR5", 35: "DDR5",
};

const STATUS_MARK: Record<CheckStatus, string> = { pass: "✓", fixable: "!", fail: "✕", unknown: "?" };

const STATUS_NAME: Record<CheckStatus, string> = {
  pass: "Đạt",
  fixable: "Sửa được trong BIOS",
  fail: "Không đạt",
  unknown: "Không đọc được",
};

const STATUS_COLOR: Record<CheckStatus, string> = {
  pass: "var(--ok)",
  fixable: "var(--warn)",
  fail: "var(--danger)",
  unknown: "var(--text-faint)",
};

const ORDER: CheckStatus[] = ["pass", "fixable", "fail", "unknown"];

function CheckRow({ c }: { c: Check }) {
  return (
    <div className="check">
      <span className="check__dot" data-s={c.status} title={STATUS_NAME[c.status]}>
        {STATUS_MARK[c.status]}
      </span>
      <div className="check__body">
        <div className="check__head">
          <span className="check__label">{c.label}</span>
          <span className="check__req">Yêu cầu: {c.requirement}</span>
        </div>
        <div className="check__value">{c.value}</div>
        {c.hint && <div className="check__hint">{c.hint}</div>}
      </div>
    </div>
  );
}

function Tally({ s }: { s: CheckSummary }) {
  const verdict = s.failed > 0
    ? "Máy không đáp ứng đủ yêu cầu của Windows 11. Bước gợi ý sẽ đề xuất phiên bản phù hợp hơn."
    : s.fixable > 0
      ? "Phần cứng đủ điều kiện, chỉ cần chỉnh vài thiết lập trong BIOS là cài được Windows 11."
      : s.unknown > 0
        ? "Không có mục nào không đạt. Vài mục không đọc được nhưng đều không phải rào chặn."
        : "Máy đáp ứng đầy đủ mọi yêu cầu của Windows 11.";

  return (
    <Panel title="Kết quả quét">
      <div className="tally">
        <div>
          <div className="tally__big">
            {s.passed}<em> / {s.total} mục đạt</em>
          </div>
        </div>
        <div className="segbar">
          {ORDER.map((k) => {
            const n = { pass: s.passed, fixable: s.fixable, fail: s.failed, unknown: s.unknown }[k];
            // max(1) chỉ để phòng chia cho 0 nếu danh sách kiểm tra rỗng.
            return n > 0
              ? <i key={k} data-s={k} style={{ width: `${(n / Math.max(1, s.total)) * 100}%` }} />
              : null;
          })}
        </div>
      </div>

      <div className="tally__verdict">{verdict}</div>

      <div className="legend">
        {ORDER.map((k) => {
          const n = { pass: s.passed, fixable: s.fixable, fail: s.failed, unknown: s.unknown }[k];
          return (
            <span key={k}>
              <b style={{ background: STATUS_COLOR[k] }} />
              {STATUS_NAME[k]} · {n}
            </span>
          );
        })}
      </div>
    </Panel>
  );
}

export function StepHardware({
  hw,
  checks,
  summary,
  loading,
  onElevate,
  onRescan,
}: {
  hw: HardwareReport | null;
  checks: Check[];
  summary: CheckSummary | null;
  loading: boolean;
  onElevate: () => void;
  onRescan: () => void;
}) {
  if (!hw) {
    return (
      <>
        <div className="main__head"><h1>Quét phần cứng</h1></div>
        <Panel>
          <Empty icon="🖥" title={loading ? "Đang quét phần cứng…" : "Chưa có dữ liệu"}>
            {loading
              ? "Đang đọc CPU, RAM, ổ đĩa, TPM, Secure Boot, firmware, đồ hoạ và màn hình."
              : "Bấm quét lại để thử lần nữa."}
          </Empty>
        </Panel>
      </>
    );
  }

  // Giữ nguyên thứ tự backend trả về; nhóm chỉ để gom lại khi hiển thị.
  const groups: string[] = [];
  for (const c of checks) if (!groups.includes(c.group)) groups.push(c.group);

  const ramMod = hw.memory_modules[0];
  const ramType = ramMod ? MEM_TYPES[ramMod.type] ?? "RAM" : "RAM";
  const machine = [hw.manufacturer, hw.model].filter(Boolean).join(" ").trim();

  return (
    <>
      <div className="main__head">
        <h1>{machine || "Quét phần cứng"}</h1>
        <p>
          Từng thành phần được đối chiếu với yêu cầu chính thức của Windows 11.
          Xanh là đạt, đỏ là không đạt, vàng là chỉ cần chỉnh thiết lập, xám là không đọc được.
        </p>
      </div>

      {summary ? (
        <Tally s={summary} />
      ) : (
        <Note type="warn" icon="!" title="Chưa đối chiếu được yêu cầu">
          Đọc được cấu hình máy nhưng chưa chấm được từng mục. Bấm quét lại ở cuối trang.
        </Note>
      )}

      {groups.map((g) => (
        <Panel key={g} title={g}>
          {checks.filter((c) => c.group === g).map((c) => <CheckRow key={c.id} c={c} />)}
        </Panel>
      ))}

      {!hw.elevated && (hw.tpm.source === "device" || hw.secure_boot_source === "registry") && (
        <Note type="info" icon="🔑" title="Đang đọc gián tiếp một số mục">
          Windows chỉ cho phép truy vấn chi tiết TPM và Secure Boot khi ứng dụng chạy với quyền
          quản trị. Ở quyền thường, ứng dụng lấy thông tin từ Device Manager và registry — đủ
          chính xác để kết luận, chỉ thiếu vài chi tiết như nhà sản xuất chip.
          <div className="actions">
            <button className="btn btn--sm" onClick={onElevate}>
              Khởi động lại với quyền quản trị
            </button>
          </div>
        </Note>
      )}

      <Panel title="Thông tin thêm">
        <div className="grid grid--2">
          <Stat k="Hệ điều hành hiện tại" v={hw.os.caption}
                note={`Bản dựng ${hw.os.build} · ${hw.os.architecture}`} />
          <Stat k="Bộ nhớ" v={`${bytes(hw.total_ram, 0)} ${ramType}`}
                note={`${ramMod?.speed ? `${ramMod.speed} MHz · ` : ""}${hw.memory_modules.length}/${hw.memory_slots || hw.memory_modules.length} khe đang dùng`} />
          <Stat k="BIOS" v={hw.bios_version || "Không rõ"}
                note={hw.system_disk.partition_style ? `Ổ hệ thống dạng ${hw.system_disk.partition_style}` : undefined} />
          <Stat k="Đồ hoạ"
                v={hw.gpus[0]?.name ?? "Không rõ"}
                note={hw.gpus.length > 1 ? `và ${hw.gpus.length - 1} card khác` : hw.gpus[0]?.driver} />
        </div>

        <div className="actions">
          <button className="btn btn--sm" onClick={onRescan} disabled={loading}>
            {loading && <span className="spinner" />} Quét lại
          </button>
        </div>
      </Panel>
    </>
  );
}
