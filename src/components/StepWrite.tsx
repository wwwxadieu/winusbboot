import { useEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { api, errorText, events } from "../lib/api";
import type {
  FormatResult, IsoInfo, PartitionScheme, SetupLanguage, UnattendConfig, UsbDisk,
  WriteProgress,
} from "../types";
import { bytes, pct, rateLine } from "../lib/format";
import { Fold, Note, Panel, Progress, Why } from "./ui";

const STAGE_NAME: Record<string, string> = {
  check: "Kiểm tra an toàn",
  partition: "Chia phân vùng",
  mount: "Gắn file ISO",
  copy: "Chép bộ cài",
  split: "Tách install.wim",
  boot: "Ghi mã khởi động",
  unattend: "Thiết lập cài đặt tự động",
  done: "Hoàn tất",
};

/**
 * Format và ghi là một việc, không phải hai.
 *
 * Trước đây chúng là hai bước riêng, và bước Format luôn dẫn thẳng tới bước Ghi
 * — không có luồng nào format xong rồi dừng lại. Tách ra chỉ tạo thêm một trang
 * phải đọc, một ô phải tick, một nút phải bấm, cộng một trạng thái hỏng người
 * dùng tự tạo ra được: format ổ này rồi đi ghi lên ổ khác. Nay một nút chạy
 * liền cả hai, và thanh tiến trình đếm chung tám chặng.
 */
const FORMAT_STAGES = 2;
const WRITE_STAGES = 6;
const ALL_STAGES = FORMAT_STAGES + WRITE_STAGES;

const SCHEMES: { id: PartitionScheme; title: string; desc: string }[] = [
  {
    id: "gpt_fat32",
    title: "GPT + FAT32 — khuyến nghị",
    desc: "Chuẩn cho mọi máy UEFI đời mới.",
  },
  {
    id: "mbr_fat32",
    title: "MBR + FAT32 — tương thích rộng",
    desc: "Boot được cả máy UEFI ở chế độ CSM lẫn máy BIOS đời cũ.",
  },
  {
    id: "mbr_ntfs",
    title: "MBR + NTFS — cho máy BIOS đời cũ",
    desc: "Không giới hạn 4 GB mỗi file, nhưng máy chỉ UEFI sẽ không nhận.",
  },
];

const TZ = [
  { id: "SE Asia Standard Time", label: "Hà Nội, Bangkok, Jakarta (GMT+7)" },
  { id: "Singapore Standard Time", label: "Singapore, Kuala Lumpur (GMT+8)" },
  { id: "Tokyo Standard Time", label: "Tokyo, Seoul (GMT+9)" },
];

const fieldStyle: CSSProperties = {
  font: "inherit", fontSize: 13, padding: "7px 11px", borderRadius: 9,
  border: "1px solid var(--border-hi)", background: "var(--glass-lo)",
  color: "var(--text)", userSelect: "text", width: "100%",
};

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label style={{ display: "block" }}>
      <span style={{ fontSize: 11, fontWeight: 600, color: "var(--text-faint)" }}>{label}</span>
      <div style={{ marginTop: 4 }}>{children}</div>
    </label>
  );
}

function Toggle({
  on, onChange, title, desc, disabled,
}: {
  on: boolean; onChange: (v: boolean) => void; title: string; desc: string; disabled?: boolean;
}) {
  return (
    <button className="opt" aria-pressed={on} disabled={disabled} onClick={() => onChange(!on)}>
      <span className="opt__radio" style={{ borderRadius: 5 }} />
      <span>
        <span className="opt__title">{title}</span>
        <span className="opt__desc">{desc}</span>
      </span>
    </button>
  );
}

export function StepWrite({
  disk,
  iso,
  admin,
  onAdminRelaunch,
  scheme,
  onScheme,
  label,
  onLabel,
  onFormatted,
  unattend,
  onUnattend,
  languages,
  isoLanguage,
  onDone,
  onDiscarded,
}: {
  disk: UsbDisk | null;
  iso: IsoInfo | null;
  admin: boolean;
  onAdminRelaunch: () => void;
  scheme: PartitionScheme;
  onScheme: (s: PartitionScheme) => void;
  label: string;
  onLabel: (v: string) => void;
  /** Ổ đã format nằm ở ký tự nào — bước cuối cần biết để chép driver vào đó. */
  onFormatted: (r: FormatResult | null) => void;
  unattend: UnattendConfig;
  onUnattend: (c: UnattendConfig) => void;
  languages: SetupLanguage[];
  /** Tên Microsoft của ngôn ngữ ISO đã chọn ở bước Phiên bản. */
  isoLanguage: string;
  /** Bước cuối chỉ mở ra khi ghi xong, nên trạng thái này phải nằm ở App. */
  onDone: (v: boolean) => void;
  /** Báo lên App rằng file ISO đã bị dọn, để bước cuối biết mà giải thích. */
  onDiscarded: () => void;
}) {
  const [phase, setPhase] = useState<null | "format" | "write">(null);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [prog, setProg] = useState<WriteProgress | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const running = phase !== null;

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
  const [preview, setPreview] = useState<string | null>(null);

  useEffect(() => {
    setDone(false);
    onDone(false);
    setError(null);
    setProg(null);
    // Ô xác nhận xoá dữ liệu không được nhớ qua một chiếc ổ khác.
    setConfirmed(false);
  }, [disk?.number, iso?.path, onDone]);

  // ISO không khởi động được UEFI thì GPT là lựa chọn vô nghĩa.
  useEffect(() => {
    if (iso && !iso.bootable_uefi && scheme === "gpt_fat32") onScheme("mbr_ntfs");
  }, [iso, scheme, onScheme]);

  // Kiến trúc trong file trả lời phải khớp bộ cài, sai là Setup bỏ qua cả file.
  useEffect(() => {
    const arch = iso?.architecture === "arm64" ? "arm64" : "amd64";
    if (unattend.arch !== arch) onUnattend({ ...unattend, arch });
  }, [iso, unattend, onUnattend]);

  const set = <K extends keyof UnattendConfig>(k: K, v: UnattendConfig[K]) =>
    onUnattend({ ...unattend, [k]: v });

  const acc = unattend.local_account;
  const isoLabel =
    languages.find((l) => l.ms_name === isoLanguage)?.label ?? isoLanguage;
  const schemeTitle = SCHEMES.find((s) => s.id === scheme)?.title.split(" — ")[0] ?? scheme;

  async function start() {
    if (!disk || !iso) return;
    setError(null);
    setDone(false);
    onDone(false);
    onFormatted(null);
    setProg(null);

    const volume = volumeName(label);
    try {
      // --- Xoá và chia lại phân vùng ---
      setPhase("format");
      // Lấy vân tay ổ ngay trước khi xoá thay vì dùng giá trị đọc từ trước:
      // nếu người dùng vừa rút ra cắm lại ổ khác, backend sẽ từ chối.
      const token = await api.diskToken(disk.number);
      const unFormat = await events.onFormatProgress(setProg);
      let formatted: FormatResult;
      try {
        formatted = await api.formatUsb({
          disk_number: disk.number,
          scheme,
          label: volume,
          confirm_token: token,
        });
      } finally {
        unFormat();
      }
      onFormatted(formatted);

      // --- Chép bộ cài lên phân vùng vừa dựng ---
      setPhase("write");
      const unWrite = await events.onWriteProgress(setProg);
      try {
        await api.writeIso({
          disk_number: disk.number,
          iso_path: iso.path,
          scheme,
          label: volume,
          confirm_token: token,
          unattend,
        });
      } finally {
        unWrite();
      }
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
    } catch (e) {
      setError(errorText(e));
    } finally {
      setPhase(null);
    }
  }

  if (!disk || !iso) {
    return (
      <>
        <div className="main__head"><h1>Ghi bộ cài</h1></div>
        <Note type="warn" icon="!">
          Cần chọn xong cả ổ USB lẫn file ISO. Hãy quay lại các bước trước.
        </Note>
      </>
    );
  }

  // Chặng đang chạy tính trên cả tám chặng của hai giai đoạn, để thanh tiến
  // trình không nhảy về 1/6 giữa chừng như thể vừa bắt đầu lại từ đầu.
  const stageIndex =
    (phase === "write" ? FORMAT_STAGES : 0) + (prog?.stage_index ?? 1);

  return (
    <>
      <div className="main__head">
        <h1>Xoá ổ và ghi bộ cài</h1>
      </div>
      <div ref={progRef} />

      {running && (
        <Panel title={`Chặng ${stageIndex}/${ALL_STAGES} · ${STAGE_NAME[prog?.stage ?? "check"] ?? ""}`}>
          <Progress
            value={prog?.percent ?? 0}
            left={prog?.message ?? "Đang bắt đầu…"}
            right={prog ? rateLine(prog) : pct(0)}
            file={prog?.detail ?? null}
            busy={(prog?.percent ?? 0) === 0}
          />
        </Panel>
      )}

      {!admin && (
        <Note type="warn" icon="🔑">
          Chia lại phân vùng và ghi ra USB đòi hỏi quyền Administrator.
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
            <div className="stat__v" style={{ fontSize: 15 }}>{disk.model}</div>
            <div className="stat__note">Ổ đĩa {disk.number} · {bytes(disk.size, 0)}</div>
          </div>
          <div className="stat">
            <div className="stat__k">Bộ cài</div>
            <div className="stat__v" style={{ fontSize: 13 }}>{iso.path.split(/[\\/]/).pop()}</div>
            <div className="stat__note">{bytes(iso.size)}</div>
          </div>
          <div className="stat">
            <div className="stat__k">Phân vùng</div>
            <div className="stat__v" style={{ fontSize: 15 }}>{schemeTitle}</div>
            <div className="stat__note">
              {volumeName(label)}
              {iso.needs_split && scheme !== "mbr_ntfs" && " · tách install.wim"}
            </div>
          </div>
        </div>
      </Panel>

      <Panel title="Cài đặt tự động sau khi khởi động máy">
        <Toggle
          on={unattend.enabled}
          disabled={running}
          onChange={(v) => set("enabled", v)}
          title="Bỏ qua các màn hình hỏi đáp ban đầu"
          desc="Trả lời sẵn phân vùng, bàn phím, mạng, giấy phép và quyền riêng tư — tiết kiệm 5–10 phút bấm chuột."
        />

        {unattend.enabled && (
          <>
            <div className="grid grid--3" style={{ marginTop: 14 }}>
              {/* Ngôn ngữ hiển thị không chọn được ở đây: nó bị khoá bởi ngôn
                  ngữ của file ISO. Đặt một ngôn ngữ không có trong ảnh đĩa thì
                  Windows Setup bỏ qua cả file trả lời. */}
              <Field label="NGÔN NGỮ HIỂN THỊ">
                <div style={{ ...fieldStyle, opacity: 0.75 }}>
                  {isoLabel} <span style={{ color: "var(--text-faint)" }}>· theo bộ cài</span>
                </div>
              </Field>

              <Field label="ĐỊNH DẠNG VÙNG">
                <select style={fieldStyle} disabled={running} value={unattend.locale}
                        onChange={(e) => {
                          const l = languages.find((x) => x.locale === e.target.value);
                          onUnattend({
                            ...unattend,
                            locale: e.target.value,
                            keyboard: l?.keyboard ?? unattend.keyboard,
                          });
                        }}>
                  {languages.map((l) => (
                    <option key={l.locale} value={l.locale}>{l.label}</option>
                  ))}
                </select>
              </Field>

              <Field label="MÚI GIỜ">
                <select style={fieldStyle} disabled={running} value={unattend.timezone}
                        onChange={(e) => set("timezone", e.target.value)}>
                  {TZ.map((t) => <option key={t.id} value={t.id}>{t.label}</option>)}
                </select>
              </Field>

              <Field label="TÊN MÁY TÍNH">
                <input style={fieldStyle} disabled={running} maxLength={15}
                       placeholder="Để trống thì Windows tự đặt"
                       value={unattend.computer_name}
                       onChange={(e) => set("computer_name", e.target.value)} />
              </Field>
            </div>

            <div style={{ marginTop: 14 }}>
              <Toggle
                on={acc !== null}
                disabled={running}
                onChange={(v) => set("local_account", v ? { name: "User", password: "", auto_logon: false } : null)}
                title="Tạo sẵn tài khoản cục bộ"
                desc="Không có mục này thì Windows 11 bắt nối mạng và đăng nhập tài khoản Microsoft trước khi vào được máy."
              />
            </div>

            {acc && (
              <div className="grid grid--3" style={{ marginTop: 12 }}>
                <Field label="TÊN TÀI KHOẢN">
                  <input style={fieldStyle} disabled={running} value={acc.name}
                         onChange={(e) => set("local_account", { ...acc, name: e.target.value })} />
                </Field>
                <Field label="MẬT KHẨU">
                  <input style={fieldStyle} disabled={running} type="password"
                         placeholder="Để trống nếu không cần"
                         value={acc.password}
                         onChange={(e) => set("local_account", { ...acc, password: e.target.value })} />
                </Field>
                <Field label="TỰ ĐĂNG NHẬP">
                  <button className="btn btn--sm" disabled={running || !acc.password}
                          style={{ width: "100%" }}
                          onClick={() => set("local_account", { ...acc, auto_logon: !acc.auto_logon })}>
                    {acc.auto_logon && acc.password ? "Đang bật" : "Đang tắt"}
                  </button>
                </Field>
              </div>
            )}

            <div style={{ marginTop: 14 }}>
              <Toggle
                on={unattend.bypass_requirements}
                disabled={running}
                onChange={(v) => set("bypass_requirements", v)}
                title="Bỏ qua kiểm tra TPM, Secure Boot và RAM"
                desc="Chỉ bật khi máy đích không đủ điều kiện Windows 11."
              />
            </div>

            {unattend.bypass_requirements && (
              <div style={{ marginTop: 12 }}>
                <Note type="warn" icon="!">
                  Máy vẫn chạy được, nhưng Microsoft không cam kết tiếp tục cấp bản cập nhật cho
                  cấu hình này. Hãy cân nhắc Windows 10 IoT Enterprise LTSC 2021 ở bước trước —
                  bản đó còn nhận bản vá bảo mật tới tháng 1/2032.
                </Note>
              </div>
            )}

            <div className="actions">
              <button className="btn btn--sm btn--ghost" disabled={running}
                      onClick={async () => {
                        setPreview(preview ? null : (await api.previewUnattend(unattend)) ?? "");
                      }}>
                {preview ? "Ẩn nội dung file" : "Xem trước autounattend.xml"}
              </button>
            </div>

            {preview && (
              <pre style={{
                marginTop: 10, maxHeight: 260, overflow: "auto", fontSize: 11,
                fontFamily: "var(--mono)", background: "var(--glass-lo)",
                border: "1px solid var(--border)", borderRadius: 12, padding: 12,
                userSelect: "text", whiteSpace: "pre", color: "var(--text-dim)",
              }}>{preview}</pre>
            )}
          </>
        )}
      </Panel>

      <Fold title="Kiểu phân vùng và tên ổ" hint={`${schemeTitle} · ${volumeName(label)}`}>
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
              File ISO này không có thư mục EFI nên chỉ boot được ở chế độ BIOS cũ.
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
      </Fold>

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

      <Note type="danger" icon="⚠">
        Ổ <b style={{ display: "inline" }}>{disk.model}</b> ({bytes(disk.size, 0)}) sẽ bị xoá
        toàn bộ và không khôi phục được.
        <label style={{ display: "flex", gap: 9, alignItems: "center", marginTop: 11, cursor: "pointer" }}>
          <input type="checkbox" checked={confirmed} disabled={running}
                 onChange={(e) => setConfirmed(e.target.checked)}
                 style={{ width: 16, height: 16, accentColor: "var(--danger)" }} />
          <span>Tôi đã sao lưu và xác nhận đúng ổ này.</span>
        </label>
      </Note>

      {error && <Note type="danger" icon="✕" title="Không ghi được">{error}</Note>}

      {discarded && (
        <Note type="info" icon="🧹">
          Đã dọn file <b style={{ display: "inline" }}>{discarded}</b>.
        </Note>
      )}

      {done && (
        <Note type="ok" icon="✓" title="USB đã sẵn sàng">
          Cắm vào máy cần cài, vào menu boot (thường là F12, F9 hoặc Esc) rồi chọn thiết bị USB.
        </Note>
      )}

      <div className="actions">
        <button className="btn btn--danger" onClick={start}
                disabled={!admin || !confirmed || running}>
          {running && <span className="spinner" />}
          {phase === "format" ? "Đang xoá ổ…"
            : phase === "write" ? "Đang ghi…"
            : done ? "Làm lại lần nữa" : "Xoá ổ và ghi bộ cài"}
        </button>
        {!confirmed && admin && !running && (
          <span style={{ fontSize: 12, color: "var(--text-faint)" }}>Hãy tick vào ô xác nhận ở trên.</span>
        )}
      </div>
    </>
  );
}

/** Tên ổ rỗng thì backend tự đặt WINSETUP — giao diện nói đúng thứ sẽ xảy ra. */
function volumeName(label: string): string {
  return label.trim() || "WINSETUP";
}
