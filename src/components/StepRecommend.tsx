import type { ReactNode } from "react";
import type {
  Candidate, CatalogOrigin, Recommendation, SetupLanguage, Verdict,
} from "../types";
import { Empty, Fold, Note, Panel, Why } from "./ui";

const ORIGIN_LABEL: Record<CatalogOrigin, string> = {
  live: "Vừa đọc từ trang của Microsoft",
  cache: "Bản lưu lần đồng bộ gần nhất",
  builtin: "Bảng nhúng sẵn trong ứng dụng",
};

const VERDICT_LABEL: Record<Verdict, string> = {
  recommended: "Cài được ngay",
  needs_setup: "Cần chỉnh BIOS",
  needs_bypass: "Phải bỏ qua kiểm tra",
  blocked: "Không cài được",
};

/** Vòng đời tính từ ngày hiện tại, nên nhãn đổi theo thời gian mà không cần sửa mã. */
function lifecycleLabel(c: Candidate): string {
  if (c.expired) return `Hết hỗ trợ ${c.end_of_support_label}`;
  if (c.days_remaining <= 120) return `Còn ${c.days_remaining} ngày`;
  return `Đến ${c.end_of_support_label}`;
}

function lifecycleTone(c: Candidate): Verdict | undefined {
  if (c.expired) return "blocked";
  if (c.days_remaining <= 120) return "needs_setup";
  if (c.days_remaining <= 365) return "needs_bypass";
  return undefined;
}

/**
 * Biểu tượng bốn ô của Windows. Dòng 11 dùng bốn ô vuông cân bằng nhau, dòng 10
 * dùng lá cờ nghiêng phối cảnh — đủ để phân biệt hai dòng ngay từ cái liếc mắt.
 */
function WindowsLogo({ family }: { family: string }) {
  const isEleven = family.includes("11");
  return (
    <svg className="rec__logo" viewBox="0 0 24 24" aria-hidden="true">
      {isEleven ? (
        <>
          <rect x="2" y="2" width="9.2" height="9.2" rx="1.1" />
          <rect x="12.8" y="2" width="9.2" height="9.2" rx="1.1" />
          <rect x="2" y="12.8" width="9.2" height="9.2" rx="1.1" />
          <rect x="12.8" y="12.8" width="9.2" height="9.2" rx="1.1" />
        </>
      ) : (
        <>
          <path d="M2 5.1 10.3 3.9v7.7H2z" />
          <path d="M11.5 3.7 22 2.2v9.4H11.5z" />
          <path d="M2 12.8h8.3v7.7L2 19.3z" />
          <path d="M11.5 12.8H22v9.4l-10.5-1.5z" />
        </>
      )}
    </svg>
  );
}

function RecCard({
  c,
  selected,
  onSelect,
}: {
  c: Candidate;
  selected: boolean;
  onSelect: () => void;
}) {
  // Chỉ nêu lý do quan trọng nhất trên thẻ: một dòng vướng mắc, hoặc một điểm
  // cộng. Liệt kê hết khiến thẻ dài gấp ba mà người dùng vẫn chỉ đọc dòng đầu.
  const headline =
    c.blockers[0] ?? c.cons[0] ?? c.pros[0] ?? c.release.tagline;
  const headlineTone = c.blockers.length ? "blk" : c.cons.length ? "con" : "pro";

  return (
    <button className="rec" aria-pressed={selected} onClick={onSelect}>
      <div className="rec__top">
        <WindowsLogo family={c.release.family} />
        <span className="rec__name">{c.release.name}</span>
        <span className="rec__score">{c.score}</span>
      </div>

      <div className="rec__tags">
        <span className="pill" data-v={c.verdict}>{VERDICT_LABEL[c.verdict]}</span>
        <span className="pill" data-v={lifecycleTone(c)}>{lifecycleLabel(c)}</span>
        {c.release.discovered && <span className="pill" data-v="needs_bypass">Mới phát hiện</span>}
        <span className="rec__build">build {c.release.build}</span>
      </div>

      <div className={`rec__line rec__line--${headlineTone}`}>
        <b>{headlineTone === "blk" ? "✕" : headlineTone === "con" ? "!" : "✓"}</b>
        {headline}
      </div>
    </button>
  );
}

export function StepRecommend({
  rec,
  loading,
  chosen,
  onChoose,
  onRefreshCatalog,
  refreshing,
  hardware,
  languages,
  language,
  onLanguage,
}: {
  rec: Recommendation | null;
  loading: boolean;
  chosen: string | null;
  onChoose: (id: string) => void;
  onRefreshCatalog: () => void;
  refreshing: boolean;
  /** Chi tiết phần cứng, gập lại ngay dưới kết luận mà nó giải thích. */
  hardware: ReactNode;
  languages: SetupLanguage[];
  /** Tên Microsoft của ngôn ngữ đang chọn, vd "English International". */
  language: string;
  onLanguage: (ms: string) => void;
}) {
  if (!rec) {
    return (
      <>
        <div className="main__head"><h1>Chọn phiên bản Windows</h1></div>
        <Panel>
          <Empty icon="🧭" title={loading ? "Đang phân tích…" : "Chưa có kết quả"}>
            {loading ? "Đang đối chiếu cấu hình máy với yêu cầu của từng bản Windows." : "Hãy quét lại phần cứng."}
          </Empty>
        </Panel>
      </>
    );
  }

  // Chỉ những ngôn ngữ thật sự tải được ISO mới lên danh sách chọn.
  const isoLangs = languages.filter((l) => !l.region_only);
  const chosenLang = isoLangs.find((l) => l.ms_name === language);

  const top = rec.candidates[0];
  const noteType =
    top?.verdict === "recommended" ? "ok" : top?.verdict === "blocked" ? "danger" : "warn";
  const s = rec.check_summary;

  return (
    <>
      <div className="main__head">
        <h1>Chọn phiên bản Windows</h1>
      </div>

      <Note type={noteType} icon="◈">{rec.summary}</Note>

      <Panel title="Xếp theo mức phù hợp với máy này">
        <div className="grid grid--2">
          {rec.candidates.map((c) => (
            <RecCard
              key={c.release.id}
              c={c}
              selected={(chosen ?? rec.best) === c.release.id}
              onSelect={() => onChoose(c.release.id)}
            />
          ))}
        </div>

        <div className="actions">
          <span className="pill" data-v={rec.catalog_origin === "live" ? "recommended" : undefined}>
            {ORIGIN_LABEL[rec.catalog_origin]}
          </span>
          {rec.catalog_synced_on && (
            <span style={{ fontSize: 12, color: "var(--text-faint)" }}>
              {rec.catalog_synced_on.split("-").reverse().join("/")}
            </span>
          )}
          <div className="spacer" />
          <button className="btn btn--sm btn--ghost" onClick={onRefreshCatalog} disabled={refreshing}>
            {refreshing && <span className="spinner" />} Đồng bộ lại
          </button>
        </div>

        {rec.catalog_origin === "builtin" && (
          <div style={{ marginTop: 12 }}>
            <Note type="warn" icon="!">
              Chưa đồng bộ được với Microsoft nên đang dùng bảng nhúng sẵn — bản Windows ra sau
              lúc ứng dụng được biên dịch sẽ không có ở đây.
            </Note>
          </div>
        )}
        {rec.catalog_note && (
          <div style={{ marginTop: 12 }}>
            <Note type="warn" icon="!">{rec.catalog_note}</Note>
          </div>
        )}
      </Panel>

      <Fold title="Chi tiết phần cứng" hint={`${s.passed}/${s.total} mục đạt · ${rec.architecture}`}>
        {hardware}
      </Fold>

      <Panel title="Bộ cài sẽ tải">
        <div className="grid grid--3">
          <div className="stat"><div className="stat__k">Kiến trúc</div><div className="stat__v">{rec.architecture}</div></div>
          <div className="stat"><div className="stat__k">Phiên bản</div><div className="stat__v">{rec.edition_hint}</div></div>
          <div className="stat">
            <div className="stat__k">Ngôn ngữ</div>
            <div style={{ marginTop: 5 }}>
              <select
                className="field"
                value={language}
                onChange={(e) => onLanguage(e.target.value)}
              >
                {isoLangs.map((l) => (
                  <option key={l.locale} value={l.ms_name}>{l.label}</option>
                ))}
              </select>
            </div>
            <div className="stat__note">{chosenLang?.locale ?? ""}</div>
          </div>
        </div>

        {/* Đây là câu hỏi người dùng Việt Nam nào cũng hỏi, nên trả lời sẵn
            ngay tại chỗ chọn thay vì để họ đi tìm rồi tự kết luận là app thiếu. */}
        <Why label="Không có tiếng Việt?">
          Microsoft chưa bao giờ phát hành ISO Windows tiếng Việt. Cách làm thông thường là
          cài một bản ở trên rồi thêm gói ngôn ngữ hiển thị sau, trong Settings → Time &amp;
          language. Riêng định dạng ngày tháng, tiền tệ và bàn phím thì đặt được thành Việt
          Nam ngay từ đầu — chọn ở bước Ghi.
        </Why>
      </Panel>
    </>
  );
}
