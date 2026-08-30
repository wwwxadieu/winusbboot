/** Các mảnh giao diện dùng lại nhiều nơi. */

import { useRef } from "react";
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
