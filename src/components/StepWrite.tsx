import { useEffect, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { api, errorText, events } from "../lib/api";
import type {
  FormatResult, IsoInfo, PartitionScheme, SetupLanguage, UnattendConfig, UsbDisk,
  WriteProgress,
} from "../types";
import { bytes, pct } from "../lib/format";
import { Note, Panel, Progress } from "./ui";

const STAGE_NAME: Record<string, string> = {
  check: "Kiểm tra an toàn",
  mount: "Gắn file ISO",
  copy: "Chép bộ cài",
  split: "Tách install.wim",
  boot: "Ghi mã khởi động",
  unattend: "Thiết lập cài đặt tự động",
  done: "Hoàn tất",
};

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
  scheme,
  label,
  format,
  unattend,
  onUnattend,
  languages,
  isoLanguage,
  onDone,
  onDiscarded,
}: {
  disk: UsbDisk | null;
  iso: IsoInfo | null;
  scheme: PartitionScheme;
  label: string;
  format: FormatResult | null;
  unattend: UnattendConfig;
  onUnattend: (c: UnattendConfig) => void;
  languages: SetupLanguage[];
  /** Tên Microsoft của ngôn ngữ ISO đã chọn ở bước Phiên bản. */
  isoLanguage: string;
  /** Bước Kiểm tra chỉ mở ra khi ghi xong, nên trạng thái này phải nằm ở App. */
  onDone: (v: boolean) => void;
  /** Báo lên App rằng file ISO đã bị dọn, để bước Kiểm tra biết mà giải thích. */
  onDiscarded: () => void;
}) {
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [prog, setProg] = useState<WriteProgress | null>(null);
  // Chỉ dọn được file do ứng dụng tự tải; file người dùng tự chọn là của họ.
  const [cleanup, setCleanup] = useState(true);
  const [discarded, setDiscarded] = useState<string | null>(null);
  const [preview, setPreview] = useState<string | null>(null);

  useEffect(() => {
    setDone(false);
    onDone(false);
    setError(null);
    setProg(null);
  }, [disk?.number, iso?.path, onDone]);

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

  async function start() {
    if (!disk || !iso) return;
    setError(null);
    setDone(false);
    setRunning(true);
    try {
      const token = await api.diskToken(disk.number);
      const un = await events.onWriteProgress(setProg);
      try {
        await api.writeIso({
          disk_number: disk.number,
          iso_path: iso.path,
          scheme,
          label: label.trim() || "WINSETUP",
          confirm_token: token,
          unattend,
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
        <div className="main__head"><h1>Ghi bộ cài</h1></div>
        <Note type="warn" icon="!">
          Cần chọn xong cả ổ USB lẫn file ISO thì mới ghi được. Hãy quay lại các bước trước.
        </Note>
      </>
    );
  }

  if (!format) {
    return (
      <>
        <div className="main__head"><h1>Ghi bộ cài</h1></div>
        <Note type="warn" icon="!" title="Chưa format ổ USB">
          Hãy quay lại bước Format và chạy xong bước đó trước. Ghi bộ cài lên một ổ chưa được
          chia lại phân vùng thì máy sẽ không khởi động được từ USB.
        </Note>
      </>
    );
  }

  return (
    <>
      <div className="main__head">
        <h1>Ghi bộ cài</h1>
        <p>Chép bộ cài lên ổ {format.drive_letter}: đã format. Mất khoảng 5–20 phút tuỳ tốc độ USB.</p>
      </div>

      <Panel title="Cài đặt tự động sau khi khởi động máy">
        <Toggle
          on={unattend.enabled}
          disabled={running}
          onChange={(v) => set("enabled", v)}
          title="Bỏ qua các màn hình hỏi đáp ban đầu"
          desc="Ghi thêm file autounattend.xml vào gốc USB. Windows Setup tự đọc file này và trả lời sẵn phần vùng, bàn phím, mạng, giấy phép, quyền riêng tư — thường tiết kiệm 5 đến 10 phút bấm chuột."
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
                desc="Bỏ qua yêu cầu đăng nhập tài khoản Microsoft. Không có mục này thì Windows 11 sẽ bắt kết nối mạng và đăng nhập trước khi vào được máy."
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
                desc="Chỉ bật khi máy đích không đủ điều kiện Windows 11. Máy vẫn cài và chạy được, nhưng Microsoft không cam kết tiếp tục cấp bản cập nhật cho cấu hình này."
              />
            </div>

            {unattend.bypass_requirements && (
              <div style={{ marginTop: 12 }}>
                <Note type="warn" icon="!">
                  Hãy cân nhắc Windows 10 IoT Enterprise LTSC 2021 ở bước gợi ý trước khi chọn
                  cách này — bản đó còn nhận bản vá bảo mật chính thức tới tháng 1/2032.
                </Note>
              </div>
            )}

            <div className="actions">
              <button className="btn btn--sm" disabled={running}
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

      {iso.managed && (
        <Panel title="Sau khi ghi xong">
          <label style={{ display: "flex", gap: 9, alignItems: "flex-start", cursor: "pointer" }}>
            <input type="checkbox" checked={cleanup} disabled={running}
                   onChange={(e) => setCleanup(e.target.checked)}
                   style={{ width: 16, height: 16, marginTop: 2, accentColor: "var(--accent)" }} />
            <span>
              <span style={{ fontWeight: 600, fontSize: 13.5, display: "block" }}>
                Xoá file ISO sau khi ghi xong
              </span>
              <span style={{ fontSize: 12.2, color: "var(--text-dim)", display: "block", marginTop: 2 }}>
                File {bytes(iso.size)} này do ứng dụng tự tải về, ghi xong là không cần nữa.
                Tắt tuỳ chọn nếu bạn muốn giữ lại để ghi thêm USB khác — hoặc để dùng chức
                năng đối chiếu từng byte ở bước Kiểm tra, vì việc đó cần chính file này.
              </span>
            </span>
          </label>
        </Panel>
      )}

      <Panel title="Sẽ ghi">
        <div className="grid grid--3">
          <div className="stat">
            <div className="stat__k">Ổ đích</div>
            <div className="stat__v">{format.drive_letter}: · {format.filesystem}</div>
            <div className="stat__note">{disk.model}</div>
          </div>
          <div className="stat">
            <div className="stat__k">Bộ cài</div>
            <div className="stat__v" style={{ fontSize: 13 }}>{iso.path.split(/[\\/]/).pop()}</div>
            <div className="stat__note">{bytes(iso.size)}</div>
          </div>
          <div className="stat">
            <div className="stat__k">Tách install.wim</div>
            <div className="stat__v">{iso.needs_split && scheme !== "mbr_ntfs" ? "Có" : "Không cần"}</div>
            <div className="stat__note">{bytes(iso.install_image_size)}</div>
          </div>
        </div>
      </Panel>

      {(running || prog) && !done && (
        <Panel title={`Bước ${prog?.stage_index ?? 1}/${prog?.total_stages ?? 6} · ${STAGE_NAME[prog?.stage ?? "check"] ?? ""}`}>
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

      {discarded && (
        <Note type="info" icon="🧹" title="Đã dọn file ISO">
          Đã xoá <b style={{ display: "inline" }}>{discarded}</b> để không chiếm dung lượng ổ đĩa.
        </Note>
      )}

      {done && (
        <Note type="ok" icon="✓" title="USB đã sẵn sàng">
          Cắm USB vào máy cần cài, vào menu boot (thường là F12, F9 hoặc Esc tuỳ hãng) và chọn
          thiết bị USB để bắt đầu.
          {unattend.enabled && " Các màn hình hỏi đáp ban đầu sẽ được trả lời tự động."}
        </Note>
      )}

      <div className="actions">
        <button className="btn btn--primary" onClick={start} disabled={running}>
          {running && <span className="spinner" />}
          {running ? "Đang ghi…" : done ? "Ghi lại lần nữa" : "Bắt đầu ghi bộ cài"}
        </button>
      </div>
    </>
  );
}
