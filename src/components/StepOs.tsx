import type { OsFamily } from "../types";
import { Why } from "./ui";

/**
 * Logo bốn ô của Windows, và logo chim cánh cụt tối giản cho Linux. Vẽ tay bằng
 * SVG thay vì tải ảnh: ứng dụng chạy trong WebView có CSP chặn ảnh ngoài, và
 * hai hình này đủ đơn giản để không cần file rời.
 */
function WindowsMark() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <rect x="2" y="2" width="9.2" height="9.2" rx="1.1" />
      <rect x="12.8" y="2" width="9.2" height="9.2" rx="1.1" />
      <rect x="2" y="12.8" width="9.2" height="9.2" rx="1.1" />
      <rect x="12.8" y="12.8" width="9.2" height="9.2" rx="1.1" />
    </svg>
  );
}

function LinuxMark() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M12 1.8c-2.5 0-4 2-4 4.6 0 1.9.3 2.7-.5 4-1.2 1.9-2.6 3.6-2.6 5.6 0 2.4 1.9 4.2 3.6 4.2 1.1 0 1.6-.5 3.5-.5s2.4.5 3.5.5c1.7 0 3.6-1.8 3.6-4.2 0-2-1.4-3.7-2.6-5.6-.8-1.3-.5-2.1-.5-4 0-2.6-1.5-4.6-4-4.6z" />
      <ellipse cx="10.1" cy="6.6" rx="1.15" ry="1.5" fill="var(--bg)" />
      <ellipse cx="13.9" cy="6.6" rx="1.15" ry="1.5" fill="var(--bg)" />
      <path d="M12 9.1c-.9 0-1.9.6-1.9 1.1 0 .4.9 1.1 1.9 1.1s1.9-.7 1.9-1.1c0-.5-1-1.1-1.9-1.1z" fill="var(--bg)" />
    </svg>
  );
}

const CHOICES: {
  id: OsFamily;
  title: string;
  sub: string;
  mark: () => JSX.Element;
  points: string[];
}[] = [
  {
    id: "windows",
    title: "Windows",
    sub: "Windows 11 · Windows 10 · các bản LTSC",
    mark: WindowsMark,
    points: [
      "Quét máy rồi gợi ý đúng bản Windows cài được",
      "Trả lời sẵn các màn hình hỏi đáp lúc cài",
      "Cần ổ USB từ 8 GB trở lên",
    ],
  },
  {
    id: "linux",
    title: "Linux",
    sub: "Ubuntu · Linux Mint · Debian · Fedora · và 5 bản khác",
    mark: LinuxMark,
    points: [
      "Gợi ý bản chạy mượt với đúng lượng RAM của máy",
      "Tải từ nguồn chính thức, tự đối chiếu mã băm",
      "Ổ USB 4 GB là đủ với phần lớn bản",
    ],
  },
];

export function StepOs({
  family,
  onChoose,
}: {
  family: OsFamily | null;
  onChoose: (f: OsFamily) => void;
}) {
  return (
    <>
      <div className="main__head">
        <h1>Bạn muốn cài gì?</h1>
      </div>

      <div className="grid grid--2">
        {CHOICES.map((c) => (
          <button
            key={c.id}
            className="oscard"
            aria-pressed={family === c.id}
            onClick={() => onChoose(c.id)}
          >
            <span className="oscard__mark" data-os={c.id}>
              {c.mark()}
            </span>
            <span className="oscard__title">{c.title}</span>
            <span className="oscard__sub">{c.sub}</span>
            <ul className="oscard__points">
              {c.points.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          </button>
        ))}
      </div>

      {family === "linux" && (
        <Why label="USB Linux khác USB Windows chỗ nào?">
          ISO Linux được ghi nguyên khối ra ổ (tương đương lệnh <code>dd</code>) vì mã khởi động
          nằm ngay trong file ảnh đĩa. Nên luồng Linux không có bước Format riêng — thao tác ghi
          đã xoá và dựng lại toàn bộ ổ — và cũng không có phần cài đặt tự động, thứ chỉ Windows
          Setup mới đọc.
        </Why>
      )}

    </>
  );
}
