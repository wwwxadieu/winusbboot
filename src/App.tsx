import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorText, events } from "./lib/api";
import { bytes } from "./lib/format";
import type {
  BootCheckRequest, DistroRecommendation, FormatResult, HardwareReport, IsoInfo, OsFamily,
  PartitionScheme, Recommendation, SetupLanguage, UnattendConfig, UsbDisk,
} from "./types";
import { Titlebar } from "./components/Titlebar";
import { Note } from "./components/ui";
import { StepOs } from "./components/StepOs";
import { StepUsb } from "./components/StepUsb";
import { StepHardware } from "./components/StepHardware";
import { StepRecommend } from "./components/StepRecommend";
import { StepDistro } from "./components/StepDistro";
import { StepSource, type SourcePlan } from "./components/StepSource";
import { StepFormat } from "./components/StepFormat";
import { StepWrite } from "./components/StepWrite";
import { StepWriteRaw } from "./components/StepWriteRaw";
import { StepVerify } from "./components/StepVerify";

type StepKey =
  | "os" | "usb" | "hardware" | "release" | "source" | "format" | "write" | "verify";

/**
 * Hai luồng khác nhau ở đúng một bước: Windows có Format riêng, Linux thì không
 * — ghi nguyên khối đã xoá và dựng lại ổ rồi (xem `writer::write_image_raw`).
 *
 * Danh sách bước vì thế là dữ liệu chứ không phải một dãy số cố định: thêm bớt
 * bước chỉ cần sửa mảng, mọi thứ khác trong file này tra theo tên bước.
 */
const WINDOWS_FLOW: StepKey[] =
  ["os", "usb", "hardware", "release", "source", "format", "write", "verify"];
const LINUX_FLOW: StepKey[] =
  ["os", "usb", "hardware", "release", "source", "write", "verify"];

function meta(key: StepKey, family: OsFamily | null): { title: string; hint: string } {
  switch (key) {
    case "os": return { title: "Hệ điều hành", hint: "Windows hay Linux" };
    case "usb": return { title: "Ổ USB", hint: "Chọn ổ để ghi" };
    case "hardware": return { title: "Phần cứng", hint: "Quét cấu hình máy" };
    case "release":
      return family === "linux"
        ? { title: "Bản Linux", hint: "Chọn bản phân phối" }
        : { title: "Phiên bản", hint: "Chọn bản Windows" };
    case "source": return { title: "Bộ cài", hint: "Chọn hoặc tải ISO" };
    case "format": return { title: "Format", hint: "Xoá và chia phân vùng" };
    case "write":
      return family === "linux"
        ? { title: "Ghi ra USB", hint: "Ghi nguyên khối" }
        : { title: "Ghi bộ cài", hint: "Chép lên USB" };
    case "verify":
      return { title: "Kiểm tra", hint: "Xác nhận boot được" };
  }
}

/** Ngôn ngữ hiển thị mặc định — phải là bản Microsoft thật sự phát hành. */
const DEFAULT_LANGUAGE = "English (United States)";

const DEFAULT_UNATTEND: UnattendConfig = {
  enabled: true,
  // Hiển thị theo ISO; định dạng vùng thì mặc định Việt Nam.
  ui_language: "en-US",
  locale: "vi-VN",
  keyboard: "0409:00000409",
  timezone: "SE Asia Standard Time",
  computer_name: "",
  local_account: { name: "User", password: "", auto_logon: false },
  skip_oobe: true,
  bypass_requirements: false,
  arch: "amd64",
};

/** Ổ nhỏ hơn mức này không chứa nổi bộ cài Windows. */
const WINDOWS_MIN_USB = 8 * 1024 ** 3;

export default function App() {
  const [theme, setTheme] = useState<"dark" | "light">(
    () => (localStorage.getItem("gwu-theme") as "dark" | "light") ?? "dark",
  );
  const [step, setStep] = useState<StepKey>("os");
  const [family, setFamily] = useState<OsFamily | null>(null);
  const [admin, setAdmin] = useState<boolean | null>(null);

  const [disks, setDisks] = useState<UsbDisk[]>([]);
  const [selectedDisk, setSelectedDisk] = useState<number | null>(null);
  const [disksLoading, setDisksLoading] = useState(true);

  const [hw, setHw] = useState<HardwareReport | null>(null);
  const [rec, setRec] = useState<Recommendation | null>(null);
  const [distros, setDistros] = useState<DistroRecommendation | null>(null);
  const [scanning, setScanning] = useState(false);
  const [refreshingCatalog, setRefreshingCatalog] = useState(false);

  const [languages, setLanguages] = useState<SetupLanguage[]>([]);
  const [language, setLanguage] = useState(DEFAULT_LANGUAGE);

  const [chosenRelease, setChosenRelease] = useState<string | null>(null);
  const [chosenDistro, setChosenDistro] = useState<string | null>(null);
  const [iso, setIso] = useState<IsoInfo | null>(null);

  const [scheme, setScheme] = useState<PartitionScheme>("gpt_fat32");
  const [label, setLabel] = useState("WINSETUP");
  const [formatResult, setFormatResult] = useState<FormatResult | null>(null);
  const [unattend, setUnattend] = useState<UnattendConfig>(DEFAULT_UNATTEND);

  const [writeDone, setWriteDone] = useState(false);
  // ISO đã bị dọn sau khi ghi: bước Kiểm tra vẫn đọc được cấu trúc ổ, nhưng
  // phần đối chiếu từng byte thì cần chính file đó nên phải giải thích.
  const [isoDiscarded, setIsoDiscarded] = useState(false);

  const [fatal, setFatal] = useState<string | null>(null);

  const elevate = useCallback(
    () => api.relaunchAsAdmin().catch((e) => setFatal(errorText(e))),
    [],
  );

  // Đổi ổ USB thì kết quả format của ổ cũ không còn giá trị.
  useEffect(() => {
    setFormatResult(null);
    setIsoDiscarded(false);
  }, [selectedDisk]);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("gwu-theme", theme);
  }, [theme]);

  // Đổi họ hệ điều hành thì file ISO đang chọn gần như chắc chắn không dùng
  // được nữa, và một ổ đã format theo chuẩn Windows cũng vô nghĩa với luồng
  // Linux. Giữ lại là mời người dùng ghi nhầm.
  const chooseFamily = useCallback((f: OsFamily) => {
    setFamily((prev) => {
      if (prev !== null && prev !== f) {
        setIso(null);
        setFormatResult(null);
      }
      return f;
    });
  }, []);

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

  // Một lần quét máy nuôi cả hai engine gợi ý — chúng đọc cùng một
  // HardwareReport, chỉ đo bằng những thước đo khác nhau.
  const scan = useCallback(async () => {
    setScanning(true);
    try {
      const [report, recommendation, distroRec] = await Promise.all([
        api.scanHardware(),
        api.getRecommendation(),
        api.recommendDistros(),
      ]);
      setHw(report);
      setRec(recommendation);
      setDistros(distroRec);
      setChosenRelease((prev) => prev ?? recommendation.best);
      setChosenDistro((prev) => prev ?? distroRec.best);
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
    api.setupLanguages().then(setLanguages).catch(() => setLanguages([]));
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
  const distro = useMemo(
    () => distros?.candidates.find((c) => c.release.id === (chosenDistro ?? distros.best))?.release ?? null,
    [distros, chosenDistro],
  );

  /** Bản đã chọn của họ hệ điều hành đang dùng — thứ các bước sau thật sự cần. */
  const picked = family === "linux" ? distro : release;

  // Ổ USB cần bao nhiêu dung lượng phụ thuộc vào thứ sắp ghi lên nó: bộ cài
  // Windows cần 8 GB, còn Lubuntu nằm gọn trong 3 GB. Ghi cứng một ngưỡng chung
  // sẽ làm mờ đi những chiếc USB hoàn toàn dùng được.
  const minUsbBytes =
    family === "linux" ? Math.max(distro?.iso_size ?? 0, 2 * 1024 ** 3) : WINDOWS_MIN_USB;

  // Ngôn ngữ hiển thị trong autounattend.xml phải khớp ngôn ngữ của ISO: đặt
  // một ngôn ngữ không có trong ảnh đĩa thì Windows Setup bỏ qua cả file.
  const uiLocale = languages.find((l) => l.ms_name === language)?.locale ?? "en-US";
  useEffect(() => {
    setUnattend((u) => (u.ui_language === uiLocale ? u : { ...u, ui_language: uiLocale }));
  }, [uiLocale]);

  const flow = family === "linux" ? LINUX_FLOW : WINDOWS_FLOW;

  const done: Record<StepKey, boolean> = {
    os: family !== null,
    usb: disk !== null,
    hardware: hw !== null,
    release: picked !== null,
    source: iso !== null,
    format: formatResult !== null,
    write: writeDone,
    verify: false,
  };

  const sub: Record<StepKey, string> = {
    os: family === null ? "Chưa chọn" : family === "linux" ? "Linux" : "Windows",
    usb: disk ? `${disk.model} · ${bytes(disk.size, 0)}` : "Chưa chọn",
    hardware: rec
      ? `${rec.check_summary.passed}/${rec.check_summary.total} mục đạt`
      : scanning ? "Đang quét…" : "Chưa quét",
    release: picked ? picked.name : "Chưa chọn",
    source: iso ? iso.path.split(/[\\/]/).pop() ?? "" : "Chưa chọn",
    format: formatResult ? `Xong · ổ ${formatResult.drive_letter}:` : "Chưa format",
    write: writeDone ? "Đã ghi xong" : admin ? "Sẵn sàng" : "Cần quyền quản trị",
    verify: writeDone ? "Sẵn sàng kiểm tra" : "Chờ ghi xong",
  };

  // Không cho nhảy tới bước sau khi bước trước chưa xong — chặn ngay ở giao diện
  // thay vì để backend từ chối sau khi người dùng đã thao tác một hồi.
  const unlocked = (k: StepKey): boolean => {
    if (k === "os") return true;
    if (family === null) return false;
    switch (k) {
      case "usb":
      case "hardware":
      case "release":
        return true;
      case "source":
        return picked !== null;
      case "format":
        return iso !== null && disk !== null;
      case "write":
        // Luồng Windows phải format xong mới ghi được; luồng Linux ghi nguyên
        // khối nên chỉ cần có ổ và có file ảnh.
        return family === "linux"
          ? iso !== null && disk !== null
          : formatResult !== null;
      case "verify":
        // Chưa ghi xong thì không có gì để đọc lại mà kiểm tra.
        return writeDone;
    }
  };

  /** Bước hiện tại có thể đã biến mất khỏi luồng sau khi người dùng đổi họ HĐH. */
  const current = flow.includes(step) ? step : "os";
  const at = flow.indexOf(current);
  const next = flow[at + 1];
  const prev = flow[at - 1];

  const bootRequest: BootCheckRequest | null = useMemo(() => {
    if (!disk || !iso || family === null) return null;
    return {
      disk_number: disk.number,
      family,
      iso_path: iso.path,
      label: label.trim() || "WINSETUP",
      // Luồng Linux không có file trả lời tự động nào để đi tìm.
      expect_unattend: family === "windows" && unattend.enabled,
    };
  }, [disk, iso, family, label, unattend.enabled]);

  // Cách lấy bộ cài là chỗ duy nhất hai họ khác nhau ở bước Nguồn, nên chỉ cần
  // gói đúng khác biệt đó lại rồi đưa cho một component dùng chung.
  const plan: SourcePlan | null = useMemo(() => {
    if (family === "linux") {
      if (!distro) return null;
      return {
        name: distro.name,
        officialPage: async () => distro.download_page,
        resolve: distro.checksum_url
          ? async () => {
              const r = await api.resolveDistroIso(distro.id);
              return { url: r.url, filename: r.filename, sha256: r.sha256 };
            }
          : null,
        manualNote: distro.checksum_url
          ? null
          : `${distro.name} không công bố link tải cố định — trang chính thức phát link theo từng phiên. Hãy tải thủ công rồi chọn file ở đây.`,
      };
    }
    if (!release) return null;
    // Tải tự động cho Windows đã tắt: Microsoft gỡ endpoint
    // /api/controls/contentinclude/html mà luồng này dựa vào — nó trả 404 với
    // mọi pageId, và trang tải hiện tại cũng không còn tham chiếu tới nó. Để
    // nút đó bật thì người dùng bấm vào chỉ nhận lỗi sau một hồi chờ, nên thà
    // nói thẳng ngay từ đầu và chỉ sang đường tải thủ công.
    //
    // Phần mã lấy link vẫn giữ nguyên trong download.rs kèm test, để bật lại
    // được ngay khi có người dựng lại luồng mới của Microsoft.
    return {
      name: release.name,
      officialPage: () => api.officialDownloadPage(release.id),
      resolve: null,
      manualNote:
        release.source === "volume_license"
          ? `${release.name} không có trên trang tải công khai của Microsoft. Bạn cần lấy ISO từ Microsoft 365 admin center, Volume Licensing Service Center, hoặc Visual Studio Subscriptions.`
          : `Microsoft đã gỡ luồng tải tự động mà ứng dụng dùng, nên nút tải tự động không còn hoạt động cho Windows. Bấm "Mở trang tải chính thức", chọn ${language}, tải file ISO về rồi quay lại đây chọn file.`,
    };
  }, [family, distro, release, language]);

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
          {flow.map((k, i) => {
            const m = meta(k, family);
            return (
              <button
                key={k}
                className="step"
                aria-current={current === k}
                data-done={done[k]}
                disabled={!unlocked(k)}
                onClick={() => setStep(k)}
              >
                <span className="step__num">{done[k] && current !== k ? "✓" : i + 1}</span>
                <span className="step__text">
                  <span className="step__title">{m.title}</span>
                  <span className="step__sub">{sub[k] || m.hint}</span>
                </span>
              </button>
            );
          })}

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

          {current === "os" && <StepOs family={family} onChoose={chooseFamily} />}

          {current === "usb" && (
            <StepUsb disks={disks} selected={selectedDisk} onSelect={setSelectedDisk}
                     onRefresh={refreshDisks} loading={disksLoading} minBytes={minUsbBytes} />
          )}

          {current === "hardware" && (
            <StepHardware hw={hw} checks={rec?.checks ?? []} summary={rec?.check_summary ?? null}
                          loading={scanning} onElevate={elevate} onRescan={scan} />
          )}

          {current === "release" && family === "linux" && (
            <StepDistro rec={distros} loading={scanning} chosen={chosenDistro}
                        onChoose={setChosenDistro} onSeeHardware={() => setStep("hardware")} />
          )}
          {current === "release" && family !== "linux" && (
            <StepRecommend rec={rec} loading={scanning} chosen={chosenRelease}
                           onChoose={setChosenRelease}
                           onRefreshCatalog={refreshCatalog} refreshing={refreshingCatalog}
                           onSeeHardware={() => setStep("hardware")}
                           languages={languages} language={language} onLanguage={setLanguage} />
          )}

          {current === "source" && (
            <StepSource family={family ?? "windows"} plan={plan} iso={iso} onIso={setIso} />
          )}

          {current === "format" && (
            <StepFormat disk={disk} iso={iso} admin={admin === true}
                        scheme={scheme} onScheme={setScheme}
                        label={label} onLabel={setLabel}
                        result={formatResult} onResult={setFormatResult}
                        onAdminRelaunch={elevate} />
          )}

          {current === "write" && family === "linux" && (
            <StepWriteRaw disk={disk} iso={iso} release={distro}
                          admin={admin === true} onAdminRelaunch={elevate}
                          onDone={setWriteDone} onDiscarded={() => setIsoDiscarded(true)} />
          )}
          {current === "write" && family !== "linux" && (
            <StepWrite disk={disk} iso={iso} scheme={scheme} label={label}
                       format={formatResult} unattend={unattend} onUnattend={setUnattend}
                       languages={languages} isoLanguage={language} onDone={setWriteDone}
                       onDiscarded={() => setIsoDiscarded(true)} />
          )}

          {current === "verify" && (
            <StepVerify request={bootRequest} writeDone={writeDone} isoDiscarded={isoDiscarded} />
          )}

          <div className="actions">
            <button className="btn" disabled={!prev} onClick={() => prev && setStep(prev)}>
              ← Quay lại
            </button>
            <div className="spacer" />
            {next && (
              <button className="btn btn--primary" disabled={!unlocked(next)}
                      onClick={() => setStep(next)}>
                Tiếp tục →
              </button>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
