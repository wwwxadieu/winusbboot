import type { Candidate, CatalogOrigin, Recommendation, Verdict } from "../types";
import { Empty, Note, Panel } from "./ui";

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
  onSeeHardware,
}: {
  rec: Recommendation | null;
  loading: boolean;
  chosen: string | null;
  onChoose: (id: string) => void;
  onRefreshCatalog: () => void;
  refreshing: boolean;
  onSeeHardware: () => void;
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

  const top = rec.candidates[0];
  const noteType =
    top?.verdict === "recommended" ? "ok" : top?.verdict === "blocked" ? "danger" : "warn";
  const s = rec.check_summary;

  return (
    <>
      <div className="main__head">
        <h1>Chọn phiên bản Windows</h1>
        <p>Điểm số phản ánh mức độ phù hợp giữa cấu hình máy và yêu cầu của từng bản, có tính cả thời gian còn được hỗ trợ.</p>
      </div>

      <Note type={noteType} icon="◈" title="Kết luận">
        {rec.summary}
        {/* Chi tiết từng thành phần nằm trọn ở bước Phần cứng — ở đây chỉ nhắc
            kết quả và đường dẫn quay lại, tránh lặp cùng một bảng hai lần. */}
        <div className="actions">
          <button className="btn btn--sm btn--ghost" onClick={onSeeHardware}>
            {s.passed}/{s.total} mục phần cứng đạt — xem chi tiết →
          </button>
        </div>
      </Note>

      <Panel title="Các phiên bản phù hợp, xếp theo điểm">
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
      </Panel>

      <Panel title="Nguồn dữ liệu phiên bản">
        <div style={{ display: "flex", alignItems: "center", gap: 12, flexWrap: "wrap" }}>
          <span className="pill" data-v={rec.catalog_origin === "live" ? "recommended" : undefined}>
            {ORIGIN_LABEL[rec.catalog_origin]}
          </span>
          {rec.catalog_synced_on && (
            <span style={{ fontSize: 12.5, color: "var(--text-dim)" }}>
              Cập nhật {rec.catalog_synced_on.split("-").reverse().join("/")}
            </span>
          )}
          <div className="spacer" />
          <button className="btn btn--sm" onClick={onRefreshCatalog} disabled={refreshing}>
            {refreshing && <span className="spinner" />} Đồng bộ lại
          </button>
        </div>

        {rec.catalog_origin === "builtin" && (
          <div style={{ marginTop: 12 }}>
            <Note type="warn" icon="!">
              Chưa đồng bộ được với Microsoft nên đang dùng bảng nhúng, đóng băng từ lúc ứng dụng
              được biên dịch. Phiên bản Windows ra sau thời điểm đó sẽ không xuất hiện ở đây.
            </Note>
          </div>
        )}
        {rec.catalog_note && (
          <div style={{ marginTop: 12 }}>
            <Note type="warn" icon="!" title="Đồng bộ chưa trọn vẹn">{rec.catalog_note}</Note>
          </div>
        )}
      </Panel>

      <Panel title="Thông số nên chọn khi tải bộ cài">
        <div className="grid grid--3">
          <div className="stat"><div className="stat__k">Kiến trúc</div><div className="stat__v">{rec.architecture}</div></div>
          <div className="stat"><div className="stat__k">Phiên bản</div><div className="stat__v">{rec.edition_hint}</div></div>
          <div className="stat"><div className="stat__k">Ngôn ngữ</div><div className="stat__v">{rec.language_hint}</div></div>
        </div>
      </Panel>
    </>
  );
}
