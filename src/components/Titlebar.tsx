import { getCurrentWindow } from "@tauri-apps/api/window";

export function Titlebar({
  admin,
  theme,
  onToggleTheme,
}: {
  admin: boolean | null;
  theme: "dark" | "light";
  onToggleTheme: () => void;
}) {
  const win = getCurrentWindow();
  return (
    <header className="titlebar">
      <div className="titlebar__drag" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <span className="brand__mark">
            <svg viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2.4"
                 strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <circle cx="12" cy="19" r="2" />
              <path d="M12 17V6M8 10l4-4 4 4" />
            </svg>
          </span>
          Get WinUSB
        </div>
        {admin !== null && (
          <span className="badge-admin" data-on={admin}>
            {admin ? "Quyền quản trị" : "Chưa có quyền quản trị"}
          </span>
        )}
      </div>

      <button className="winbtn" onClick={onToggleTheme}
              title={theme === "dark" ? "Chuyển sang giao diện sáng" : "Chuyển sang giao diện tối"}>
        {theme === "dark" ? "☾" : "☀"}
      </button>

      <div className="winbtns">
        <button className="winbtn" onClick={() => win.minimize()} title="Thu nhỏ">─</button>
        <button className="winbtn" onClick={() => win.toggleMaximize()} title="Phóng to">☐</button>
        <button className="winbtn winbtn--close" onClick={() => win.close()} title="Đóng">✕</button>
      </div>
    </header>
  );
}
