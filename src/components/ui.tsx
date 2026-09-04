/** Các mảnh giao diện dùng lại nhiều nơi. */

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { ReactNode } from "react";
import { pct as fmtPct } from "../lib/format";

export function Panel({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="panel">
      {title && <div className="panel__title">{title}</div>}
      {children}
    </section>
  );
}

export function Note({
  type = "info",
  icon,
  title,
  children,
}: {
  type?: "info" | "warn" | "danger" | "ok";
  icon?: string;
  title?: string;
  children: ReactNode;
}) {
  const fallback = { info: "i", warn: "!", danger: "!", ok: "✓" }[type];
  return (
    <div className="note" data-t={type}>
      <span className="note__icon">{icon ?? fallback}</span>
      <div>
        {title && <b>{title}</b>}
        {children}
      </div>
    </div>
  );
}

/**
 * Chỗ giấu phần giải thích dài.
 *
 * Ứng dụng có nhiều đoạn cần nói — vì sao không có ISO tiếng Việt, vì sao ghi
 * nguyên khối không cần format, vì sao đọc lại từng byte mới bắt được USB dỏm.
 * Để tất cả nằm phơi trên trang thì người dùng phải đọc hết mới tìm ra nút bấm;
 * bỏ đi thì lần đầu gặp lạ họ không có gì để tra. Đóng lại theo mặc định là
 * đường giữa: trang ngắn cho người đã biết, câu trả lời vẫn ở ngay chỗ nảy ra
 * câu hỏi cho người chưa biết.
 */
export function Why({ label = "Vì sao?", children }: { label?: string; children: ReactNode }) {
  return (
    <details className="why">
      <summary>{label}</summary>
      <div className="why__body">{children}</div>
    </details>
  );
}

/** Một khung như `Panel` nhưng gập lại được — dùng cho phần không phải ai cũng cần. */
export function Fold({
  title,
  hint,
  open,
  children,
}: {
  title: string;
  hint?: ReactNode;
  open?: boolean;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDetailsElement>(null);

  // Khối gập hay nằm cuối trang, và mở ra thì phần bung ra rơi hết xuống dưới
  // mép màn hình: người dùng bấm xong nhìn vào một trang không đổi gì và tưởng
  // nút hỏng. Kéo theo đúng một lần, lúc mở.
  //
  // Nội dung cao hơn khung nhìn thì căn theo mép trên — căn mép dưới sẽ đẩy
  // tiêu đề vừa bấm ra khỏi màn hình. Ngắn hơn thì kéo tối thiểu, để trang
  // không giật lên khi thứ cần xem vốn đã nằm trong tầm mắt.
  const onToggle = () => {
    const el = ref.current;
    if (!el?.open) return;
    requestAnimationFrame(() => {
      const tall = el.getBoundingClientRect().height > window.innerHeight * 0.8;
      el.scrollIntoView({ behavior: "smooth", block: tall ? "start" : "nearest" });
    });
  };

  return (
    <details className="fold" open={open} ref={ref} onToggle={onToggle}>
      <summary>
        <span className="fold__title">{title}</span>
        {hint && <span className="fold__hint">{hint}</span>}
      </summary>
      <div className="fold__body">{children}</div>
    </details>
  );
}

export interface Option {
  value: string;
  label: string;
}

/**
 * Ô chọn thay cho `<select>` gốc.
 *
 * Danh sách xổ ra của `<select>` gốc do WebView2 vẽ, không phải trang web — CSS
 * của ứng dụng không với tới được nó. Nên dù ô đóng có kiểu dáng kính đúng như
 * phần còn lại, vừa bấm mở ra là hiện một bảng trắng vuông vức của hệ điều
 * hành, lạc hẳn khỏi mọi thứ quanh nó.
 *
 * Bảng chọn phải nằm trong một **portal ra thẳng `body`**, không phải cạnh ô.
 * `.main__scroll` có `overflow-y: auto`, nên một bảng đặt trong luồng sẽ bị cắt
 * ở mép khối cuộn, và cuộn trang thì nó trôi theo. Đổi lại, vị trí phải tự đo:
 * `position: fixed` theo toạ độ của ô, lật lên trên khi phía dưới không đủ chỗ.
 *
 * Cuộn hay đổi cỡ cửa sổ thì đóng lại thay vì đo lại liên tục — bảng chọn là
 * thứ tồn tại vài giây, và đóng nó đi rẻ hơn nhiều so với việc bám đuổi một ô
 * đang chạy khỏi nó.
 */
export function Select({
  value,
  onChange,
  options,
  disabled,
  label,
}: {
  value: string;
  onChange: (v: string) => void;
  options: Option[];
  disabled?: boolean;
  /** Nhãn cho trình đọc màn hình, vì ô này không phải `<select>` thật. */
  label?: string;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [box, setBox] = useState<{ left: number; top: number; width: number; drop: boolean } | null>(null);

  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);

  const current = options.find((o) => o.value === value);

  // Đo ngay trước khi trình duyệt vẽ, để bảng không nhấp nháy ở vị trí cũ.
  useLayoutEffect(() => {
    if (!open) return setBox(null);
    const el = trigger.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const below = window.innerHeight - r.bottom;
    // Đủ chỗ bên dưới thì xổ xuống; không thì lật lên trên. Ngưỡng lấy theo
    // chiều cao tối đa của bảng cộng khoảng hở, chứ không phải một nửa màn
    // hình: ô nằm hơi dưới giữa vẫn xổ xuống được nếu còn đủ chỗ.
    const drop = below >= 240 || below >= r.top;
    setBox({ left: r.left, top: drop ? r.bottom + 6 : r.top - 6, width: r.width, drop });
  }, [open]);

  // Mở ra là con trỏ nằm sẵn ở mục đang chọn, không phải ở đầu danh sách: với
  // danh sách mười mấy bản Windows, bắt đầu từ đầu nghĩa là mỗi lần mở lại phải
  // dò lại từ đầu.
  useEffect(() => {
    if (open) setActive(Math.max(0, options.findIndex((o) => o.value === value)));
  }, [open, options, value]);

  useEffect(() => {
    if (!open) return;

    const onPointer = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!menu.current?.contains(t) && !trigger.current?.contains(t)) setOpen(false);
    };
    const onLeave = () => setOpen(false);

    // `true` để bắt được cả cuộn bên trong `.main__scroll`, vì sự kiện scroll
    // không nổi bọt lên window.
    document.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("scroll", onLeave, true);
    window.addEventListener("resize", onLeave);
    return () => {
      document.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("scroll", onLeave, true);
      window.removeEventListener("resize", onLeave);
    };
  }, [open]);

  // Giữ mục đang trỏ luôn nằm trong tầm nhìn khi đi bằng bàn phím.
  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLElement>('[data-active="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [open, active]);

  const pick = (v: string) => {
    onChange(v);
    setOpen(false);
    trigger.current?.focus();
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (disabled) return;

    if (!open) {
      if (["Enter", " ", "ArrowDown", "ArrowUp"].includes(e.key)) {
        e.preventDefault();
        setOpen(true);
      }
      return;
    }

    switch (e.key) {
      case "Escape":
        e.preventDefault();
        setOpen(false);
        break;
      case "Enter":
      case " ":
        e.preventDefault();
        if (options[active]) pick(options[active].value);
        break;
      case "ArrowDown":
        e.preventDefault();
        setActive((i) => Math.min(options.length - 1, i + 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setActive((i) => Math.max(0, i - 1));
        break;
      case "Home":
        e.preventDefault();
        setActive(0);
        break;
      case "End":
        e.preventDefault();
        setActive(options.length - 1);
        break;
    }
  };

  return (
    <div className="select">
      <button
        ref={trigger}
        type="button"
        className="field select__trigger"
        role="combobox"
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-label={label}
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        onKeyDown={onKey}
      >
        <span className="select__value">{current?.label ?? ""}</span>
        <span className="select__chev" aria-hidden="true" data-open={open} />
      </button>

      {open && box &&
        createPortal(
          <div
            ref={menu}
            className="select__menu"
            role="listbox"
            style={{
              left: box.left,
              width: box.width,
              ...(box.drop ? { top: box.top } : { bottom: window.innerHeight - box.top }),
            }}
            onKeyDown={onKey}
          >
            {options.map((o, i) => (
              <button
                key={o.value}
                type="button"
                role="option"
                aria-selected={o.value === value}
                className="select__opt"
                data-active={i === active}
                data-on={o.value === value}
                onPointerEnter={() => setActive(i)}
                onClick={() => pick(o.value)}
              >
                {o.label}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </div>
  );
}

export function Bar({ value, busy = false }: { value: number; busy?: boolean }) {
  return (
    <div className={busy ? "bar bar--busy" : "bar"}>
      <div className="bar__fill" style={{ width: `${Math.min(100, Math.max(0, value))}%` }} />
    </div>
  );
}

export function Progress({
  value,
  left,
  right,
  file,
  busy,
}: {
  value: number;
  left: string;
  right?: string;
  file?: string | null;
  busy?: boolean;
}) {
  return (
    <div>
      <Bar value={value} busy={busy} />
      <div className="progress-meta">
        <span>{left}</span>
        <span>{right ?? fmtPct(value)}</span>
      </div>
      {file && <div className="progress-file">{file}</div>}
    </div>
  );
}

export function Stat({ k, v, note }: { k: string; v: ReactNode; note?: ReactNode }) {
  return (
    <div className="stat">
      <div className="stat__k">{k}</div>
      <div className="stat__v">{v}</div>
      {note && <div className="stat__note">{note}</div>}
    </div>
  );
}

export function Empty({ icon, title, children }: { icon: string; title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <div className="empty__icon">{icon}</div>
      <b>{title}</b>
      {children}
    </div>
  );
}

export function UsbIcon() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="2"
         strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="19" r="2.2" />
      <path d="M12 16.8V7" />
      <path d="M7.5 11.5 12 7l4.5 4.5" />
      <rect x="9.4" y="2.2" width="5.2" height="4.4" rx="1.2" />
    </svg>
  );
}
