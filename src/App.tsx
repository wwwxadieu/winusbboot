import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorText, events } from "./lib/api";
import { bytes } from "./lib/format";
import type {
  FormatResult, HardwareReport, IsoInfo, PartitionScheme, Recommendation,
  UnattendConfig, UsbDisk,
} from "./types";
import { Titlebar } from "./components/Titlebar";
import { Note } from "./components/ui";
import { StepUsb } from "./components/StepUsb";
import { StepHardware } from "./components/StepHardware";
import { StepRecommend } from "./components/StepRecommend";
import { StepSource } from "./components/StepSource";
import { StepFormat } from "./components/StepFormat";
import { StepWrite } from "./components/StepWrite";

type StepId = 0 | 1 | 2 | 3 | 4 | 5;

const STEPS = [
  { title: "Ổ USB", hint: "Chọn ổ để ghi" },
  { title: "Phần cứng", hint: "Quét cấu hình máy" },
  { title: "Phiên bản", hint: "Chọn bản Windows" },
  { title: "Bộ cài", hint: "Chọn hoặc tải ISO" },
  { title: "Format", hint: "Xoá và chia phân vùng" },
  { title: "Ghi bộ cài", hint: "Chép lên USB" },
];

const DEFAULT_UNATTEND: UnattendConfig = {
  enabled: true,
  language: "vi-VN",
  keyboard: "0409:00000409",
  timezone: "SE Asia Standard Time",
  computer_name: "",
  local_account: { name: "User", password: "", auto_logon: false },
  skip_oobe: true,
  bypass_requirements: false,
  arch: "amd64",
};

export default function App() {
  const [theme, setTheme] = useState<"dark" | "light">(
    () => (localStorage.getItem("gwu-theme") as "dark" | "light") ?? "dark",
  );
  const [step, setStep] = useState<StepId>(0);
  const [admin, setAdmin] = useState<boolean | null>(null);

  const [disks, setDisks] = useState<UsbDisk[]>([]);
  const [selectedDisk, setSelectedDisk] = useState<number | null>(null);
  const [disksLoading, setDisksLoading] = useState(true);

  const [hw, setHw] = useState<HardwareReport | null>(null);
  const [rec, setRec] = useState<Recommendation | null>(null);
  const [scanning, setScanning] = useState(false);
  const [refreshingCatalog, setRefreshingCatalog] = useState(false);

  const [chosenRelease, setChosenRelease] = useState<string | null>(null);
  const [iso, setIso] = useState<IsoInfo | null>(null);

  const [scheme, setScheme] = useState<PartitionScheme>("gpt_fat32");
  const [label, setLabel] = useState("WINSETUP");
  const [formatResult, setFormatResult] = useState<FormatResult | null>(null);
  const [unattend, setUnattend] = useState<UnattendConfig>(DEFAULT_UNATTEND);

  const [fatal, setFatal] = useState<string | null>(null);

  const elevate = useCallback(
    () => api.relaunchAsAdmin().catch((e) => setFatal(errorText(e))),
    [],
  );

  // Đổi ổ USB thì kết quả format của ổ cũ không còn giá trị.
  useEffect(() => {
    setFormatResult(null);
  }, [selectedDisk]);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("gwu-theme", theme);
  }, [theme]);

  const refreshDisks = useCallback(async () => {
    setDisksLoading(true);
    try {
      setDisks(await api.listUsbDisks());
    } catch (e) {
      setFatal(errorText(e));
    } finally {
      setDisksLoading(false);
    }
  }, []);

  const scan = useCallback(async () => {
    setScanning(true);
    try {
      const [report, recommendation] = await Promise.all([
        api.scanHardware(),
        api.getRecommendation(),
      ]);
      setHw(report);
      setRec(recommendation);
      setChosenRelease((prev) => prev ?? recommendation.best);
    } catch (e) {
      setFatal(errorText(e));
    } finally {
      setScanning(false);
    }
  }, []);

  // Danh mục vừa đồng bộ xong thì phải chấm điểm lại: có thể vừa xuất hiện một
  // bản Windows mới, hoặc mốc hết hỗ trợ vừa đổi.
  const rescore = useCallback(async () => {
    try {
      setRec(await api.getRecommendation());
    } catch (e) {
      setFatal(errorText(e));
    }
  }, []);

  const refreshCatalog = useCallback(async () => {
    setRefreshingCatalog(true);
    try {
      await api.refreshCatalog();
    } catch (e) {
      setFatal(errorText(e));
    } finally {
      await rescore();
      setRefreshingCatalog(false);
    }
  }, [rescore]);

  useEffect(() => {
    api.isAdmin().then(setAdmin).catch(() => setAdmin(false));
    refreshDisks();
    scan();

    let unCatalog: (() => void) | undefined;
    events.onCatalogUpdated(() => { void rescore(); }).then((f) => { unCatalog = f; });

    let un: (() => void) | undefined;
    events.onUsbChanged((list) => {
      setDisks(list);
      setDisksLoading(false);
      // Ổ đang chọn bị rút ra thì bỏ chọn để không ghi nhầm ổ khác cùng số.
      setSelectedDisk((cur) => (cur !== null && !list.some((d) => d.number === cur) ? null : cur));
    }).then((f) => { un = f; });

    return () => { un?.(); unCatalog?.(); };
  }, [refreshDisks, scan, rescore]);

  const disk = useMemo(
    () => disks.find((d) => d.number === selectedDisk) ?? null,
    [disks, selectedDisk],
  );
  const release = useMemo(
    () => rec?.candidates.find((c) => c.release.id === (chosenRelease ?? rec.best))?.release ?? null,
    [rec, chosenRelease],
  );

  const doneMap: Record<StepId, boolean> = {
    0: disk !== null,
    1: hw !== null,
    2: release !== null,
    3: iso !== null,
    4: formatResult !== null,
    5: false,
  };
  const subMap: Record<StepId, string> = {
    0: disk ? `${disk.model} · ${bytes(disk.size, 0)}` : "Chưa chọn",
    1: rec
      ? `${rec.check_summary.passed}/${rec.check_summary.total} mục đạt`
      : scanning ? "Đang quét…" : "Chưa quét",
    2: release ? release.name : "Chưa chọn",
    3: iso ? iso.path.split(/[\\/]/).pop() ?? "" : "Chưa chọn",
    4: formatResult ? `Xong · ổ ${formatResult.drive_letter}:` : "Chưa format",
    5: admin ? "Sẵn sàng" : "Cần quyền quản trị",
  };

  // Không cho nhảy tới bước sau khi bước trước chưa xong — chặn ngay ở giao diện
  // thay vì để backend từ chối sau khi người dùng đã thao tác một hồi.
  const unlocked = (s: StepId) =>
    s <= 2
    || (s === 3 && release !== null)
    || (s === 4 && iso !== null && disk !== null)
    || (s === 5 && formatResult !== null);

  const canNext = step < 5 && unlocked((step + 1) as StepId);

  return (
    <div className="app">
      <Titlebar
        admin={admin}
        theme={theme}
        onToggleTheme={() => setTheme((t) => (t === "dark" ? "light" : "dark"))}
      />

      <div className="body">
        <nav className="rail">
          <div className="rail__label">Các bước</div>
          {STEPS.map((s, i) => (
            <button
              key={i}
              className="step"
              aria-current={step === i}
              data-done={doneMap[i as StepId]}
              disabled={!unlocked(i as StepId)}
              onClick={() => setStep(i as StepId)}
            >
              <span className="step__num">{doneMap[i as StepId] && step !== i ? "✓" : i + 1}</span>
              <span className="step__text">
                <span className="step__title">{s.title}</span>
                <span className="step__sub">{subMap[i as StepId] || s.hint}</span>
              </span>
            </button>
          ))}

          <div className="rail__foot">
            <button className="btn btn--sm btn--ghost" onClick={scan} disabled={scanning}>
              {scanning && <span className="spinner" />} Quét lại máy
            </button>
          </div>
        </nav>

        <main className="main">
          {fatal && (
            <Note type="danger" icon="✕" title="Không đọc được thông tin hệ thống">
              {fatal}
            </Note>
          )}

          {step === 0 && (
            <StepUsb disks={disks} selected={selectedDisk} onSelect={setSelectedDisk}
                     onRefresh={refreshDisks} loading={disksLoading} />
          )}
          {step === 1 && (
            <StepHardware hw={hw} checks={rec?.checks ?? []} summary={rec?.check_summary ?? null}
                          loading={scanning} onElevate={elevate} onRescan={scan} />
          )}
          {step === 2 && (
            <StepRecommend rec={rec} loading={scanning} chosen={chosenRelease}
                           onChoose={setChosenRelease}
                           onRefreshCatalog={refreshCatalog} refreshing={refreshingCatalog}
                           onSeeHardware={() => setStep(1)} />
          )}
          {step === 3 && <StepSource release={release} iso={iso} onIso={setIso} />}
          {step === 4 && (
            <StepFormat disk={disk} iso={iso} admin={admin === true}
                        scheme={scheme} onScheme={setScheme}
                        label={label} onLabel={setLabel}
                        result={formatResult} onResult={setFormatResult}
                        onAdminRelaunch={elevate} />
          )}
          {step === 5 && (
            <StepWrite disk={disk} iso={iso} scheme={scheme} label={label}
                       format={formatResult} unattend={unattend} onUnattend={setUnattend} />
          )}

          <div className="actions">
            <button className="btn" disabled={step === 0}
                    onClick={() => setStep((s) => Math.max(0, s - 1) as StepId)}>
              ← Quay lại
            </button>
            <div className="spacer" />
            {step < 5 && (
              <button className="btn btn--primary" disabled={!canNext}
                      onClick={() => setStep((s) => Math.min(5, s + 1) as StepId)}>
                Tiếp tục →
              </button>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
