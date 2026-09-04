import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorText, events } from "./lib/api";
import { bytes } from "./lib/format";
import type {
  BootCheckRequest, DistroRecommendation, FormatResult, HardwareReport, IsoInfo, OsFamily,
  PartitionScheme, Recommendation, SetupLanguage, StageReport, UnattendConfig, UsbDisk,
} from "./types";
import { Titlebar } from "./components/Titlebar";
import { Note } from "./components/ui";
import { StepOs } from "./components/StepOs";
import { StepUsb, usable } from "./components/StepUsb";
import { HardwareBlock } from "./components/StepHardware";
import { StepRecommend } from "./components/StepRecommend";
import { StepDistro } from "./components/StepDistro";
import { StepSource, type SourcePlan } from "./components/StepSource";
import { StepWrite } from "./components/StepWrite";
import { StepWriteRaw } from "./components/StepWriteRaw";
import { StepFinish } from "./components/StepFinish";

type StepKey = "os" | "usb" | "release" | "source" | "write" | "finish";

/**
 * Một luồng duy nhất, sáu bước, dùng chung cho cả Windows lẫn Linux.
 *
 * Trước đây có hai mảng khác nhau vì Windows có bước Format riêng còn Linux thì
 * không. Nay Format nằm trong chính thao tác ghi ở cả hai họ, nên khác biệt
 * giữa chúng rút gọn lại thành: bước nào dựng component nào. Bốn bước từng
 * đứng riêng — Phần cứng, Format, Driver, Kiểm tra — đều không phải chỗ người
 * dùng ra quyết định, nên chúng nằm gọn bên trong bước mà chúng phục vụ.
 */
const FLOW: StepKey[] = ["os", "usb", "release", "source", "write", "finish"];

function meta(key: StepKey, family: OsFamily | null): { title: string; hint: string } {
  switch (key) {
    case "os": return { title: "Hệ điều hành", hint: "Windows hay Linux" };
    case "usb": return { title: "Ổ USB", hint: "Chọn ổ để ghi" };
    case "release":
      return family === "linux"
        ? { title: "Bản Linux", hint: "Chọn bản phân phối" }
        : { title: "Phiên bản", hint: "Chọn bản Windows" };
    case "source": return { title: "Bộ cài", hint: "Chọn hoặc tải ISO" };
    case "write":
      return family === "linux"
        ? { title: "Ghi ra USB", hint: "Ghi nguyên khối" }
        : { title: "Ghi bộ cài", hint: "Xoá ổ rồi chép lên" };
    case "finish": return { title: "Xong", hint: "Kiểm tra lại USB" };
  }
}

/** Ngôn ngữ hiển thị mặc định — phải khớp đúng tên Microsoft dùng trong API tải. */
const DEFAULT_LANGUAGE = "English";

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
  // Không chọn hộ bản Windows: một ISO multi-edition chứa tới mười bản, và
  // đoán hộ ở đó là đoán xem người dùng định cài bản nào.
  edition: "",
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
  // Kết quả format nay do bước Ghi tạo ra; giữ ở đây vì phần kèm driver ở bước
  // cuối cần biết bộ cài nằm ở ký tự ổ nào.
  const [formatResult, setFormatResult] = useState<FormatResult | null>(null);
  const [unattend, setUnattend] = useState<UnattendConfig>(DEFAULT_UNATTEND);

  const [writeDone, setWriteDone] = useState(false);
  const [staged, setStaged] = useState<StageReport | null>(null);
  // ISO đã bị dọn sau khi ghi: bước cuối vẫn đọc được cấu trúc ổ, nhưng phần
  // đối chiếu từng byte thì cần chính file đó nên phải giải thích.
  const [isoDiscarded, setIsoDiscarded] = useState(false);

  const [fatal, setFatal] = useState<string | null>(null);

  const elevate = useCallback(
    () => api.relaunchAsAdmin().catch((e) => setFatal(errorText(e))),
    [],
  );

  // Đổi ổ USB thì kết quả của ổ cũ không còn giá trị.
  useEffect(() => {
    setFormatResult(null);
    setIsoDiscarded(false);
    setStaged(null);
  }, [selectedDisk]);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("gwu-theme", theme);
  }, [theme]);

  // Đổi họ hệ điều hành thì file ISO đang chọn gần như chắc chắn không dùng
  // được nữa. Giữ lại là mời người dùng ghi nhầm.
  //
  // Chọn xong là đi luôn sang bước sau: đây là câu hỏi có đúng hai đáp án, và
  // bấm "Tiếp tục" ngay sau khi vừa bấm "Windows" chỉ là bắt xác nhận hai lần
  // một việc.
  const chooseFamily = useCallback((f: OsFamily) => {
    setFamily((prev) => {
      if (prev !== null && prev !== f) {
        setIso(null);
        setFormatResult(null);
      }
      return f;
    });
    setStep("usb");
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

  // Cắm đúng một chiếc USB dùng được thì không có gì để chọn — chọn sẵn hộ.
  // Có từ hai ổ trở lên thì không: đoán hộ ở đó là đoán xem ổ nào bị xoá.
  useEffect(() => {
    if (selectedDisk !== null) return;
    const fit = disks.filter((d) => usable(d, minUsbBytes));
    if (fit.length === 1) setSelectedDisk(fit[0].number);
  }, [disks, minUsbBytes, selectedDisk]);

  // Ngôn ngữ hiển thị trong autounattend.xml phải khớp ngôn ngữ của ISO: đặt
  // một ngôn ngữ không có trong ảnh đĩa thì Windows Setup bỏ qua cả file.
  const uiLocale = languages.find((l) => l.ms_name === language)?.locale ?? "en-US";
  useEffect(() => {
    setUnattend((u) => (u.ui_language === uiLocale ? u : { ...u, ui_language: uiLocale }));
  }, [uiLocale]);

  // `picked` có giá trị ngay từ đầu vì bảng gợi ý đã chấm điểm xong lúc khởi
  // động. Nhưng chưa chọn hệ điều hành thì đó chỉ là mặc định của bảng Windows,
  // không phải lựa chọn của người dùng — đánh dấu xong ở đó là nói dối.
  const done: Record<StepKey, boolean> = {
    os: family !== null,
    usb: disk !== null,
    release: family !== null && picked !== null,
    source: iso !== null,
    write: writeDone,
    finish: false,
  };

  const sub: Record<StepKey, string> = {
    os: family === null ? "Chưa chọn" : family === "linux" ? "Linux" : "Windows",
    usb: disk ? `${disk.model} · ${bytes(disk.size, 0)}` : "Chưa chọn",
    release: family === null
      ? "Chưa chọn"
      : picked ? picked.name
      : scanning ? "Đang quét máy…" : "Chưa chọn",
    source: iso ? iso.path.split(/[\\/]/).pop() ?? "" : "Chưa chọn",
    write: writeDone ? "Đã ghi xong" : admin ? "Sẵn sàng" : "Cần quyền quản trị",
    finish: writeDone
      ? staged ? `Đã kèm ${staged.packages} gói driver` : "Kiểm tra lại USB"
      : "Chờ ghi xong",
  };

  // Không cho nhảy tới bước sau khi bước trước chưa xong — chặn ngay ở giao diện
  // thay vì để backend từ chối sau khi người dùng đã thao tác một hồi.
  const unlocked = (k: StepKey): boolean => {
    if (k === "os") return true;
    if (family === null) return false;
    switch (k) {
      case "usb":
      case "release":
        return true;
      case "source":
        return picked !== null;
      case "write":
        // Bước ghi nay tự xoá và chia lại phân vùng, nên chỉ cần có ổ và có
        // file ảnh là đủ điều kiện — ở cả hai họ hệ điều hành.
        return iso !== null && disk !== null;
      case "finish":
        // Chưa ghi xong thì không có gì để đọc lại mà kiểm tra.
        return writeDone;
    }
  };

  const at = FLOW.indexOf(step);
  const next = FLOW[at + 1];
  const prev = FLOW[at - 1];

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
    // Bản doanh nghiệp không nằm trên trang tải công khai, nên không có gì để
    // hỏi Microsoft cả — chỉ còn đường lấy từ kênh bản quyền.
    if (release.source === "volume_license") {
      return {
        name: release.name,
        officialPage: () => api.officialDownloadPage(release.id),
        resolve: null,
        manualNote: `${release.name} không có trên trang tải công khai của Microsoft. Bạn cần lấy ISO từ Microsoft 365 admin center, Volume Licensing Service Center, hoặc Visual Studio Subscriptions.`,
      };
    }
    return {
      name: release.name,
      officialPage: () => api.officialDownloadPage(release.id),
      resolve: async () => {
        const r = await api.resolveWindowsIso(release.id, language);
        return { url: r.url, filename: r.filename, sha256: r.sha256 };
      },
      manualNote: null,
    };
  }, [family, distro, release, language]);

  /** Chi tiết phần cứng, dùng chung cho cả hai bảng gợi ý. */
  const hardware = (
    <HardwareBlock hw={hw} checks={rec?.checks ?? []} summary={rec?.check_summary ?? null}
                   loading={scanning} onElevate={elevate} onRescan={scan} />
  );

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
          {FLOW.map((k, i) => {
            const m = meta(k, family);
            return (
              <button
                key={k}
                className="step"
                aria-current={step === k}
                data-done={done[k]}
                disabled={!unlocked(k)}
                onClick={() => setStep(k)}
              >
                <span className="step__num">{done[k] && step !== k ? "✓" : i + 1}</span>
                <span className="step__text">
                  <span className="step__title">{m.title}</span>
                  <span className="step__sub">{sub[k] || m.hint}</span>
                </span>
              </button>
            );
          })}
        </nav>

        <main className="main">
          <div className="main__scroll">
            <div className="shell">
              {fatal && (
                <Note type="danger" icon="✕" title="Không đọc được thông tin hệ thống">
                  {fatal}
                </Note>
              )}

              {step === "os" && <StepOs family={family} onChoose={chooseFamily} />}

              {step === "usb" && (
                <StepUsb disks={disks} selected={selectedDisk} onSelect={setSelectedDisk}
                         onRefresh={refreshDisks} loading={disksLoading} minBytes={minUsbBytes} />
              )}

              {step === "release" && family === "linux" && (
                <StepDistro rec={distros} loading={scanning} chosen={chosenDistro}
                            onChoose={setChosenDistro} hardware={hardware} />
              )}
              {step === "release" && family !== "linux" && (
                <StepRecommend rec={rec} loading={scanning} chosen={chosenRelease}
                               onChoose={setChosenRelease}
                               onRefreshCatalog={refreshCatalog} refreshing={refreshingCatalog}
                               hardware={hardware}
                               languages={languages} language={language} onLanguage={setLanguage} />
              )}

              {step === "source" && (
                <StepSource family={family ?? "windows"} plan={plan} iso={iso} onIso={setIso} />
              )}

              {step === "write" && family === "linux" && (
                <StepWriteRaw disk={disk} iso={iso} release={distro}
                              admin={admin === true} onAdminRelaunch={elevate}
                              onDone={setWriteDone} onDiscarded={() => setIsoDiscarded(true)} />
              )}
              {step === "write" && family !== "linux" && (
                <StepWrite disk={disk} iso={iso} admin={admin === true} onAdminRelaunch={elevate}
                           scheme={scheme} onScheme={setScheme} label={label} onLabel={setLabel}
                           onFormatted={setFormatResult}
                           unattend={unattend} onUnattend={setUnattend}
                           languages={languages} isoLanguage={language} onDone={setWriteDone}
                           onDiscarded={() => setIsoDiscarded(true)} />
              )}

              {step === "finish" && (
                <StepFinish family={family ?? "windows"} request={bootRequest}
                            writeDone={writeDone} isoDiscarded={isoDiscarded}
                            driveLetter={formatResult?.drive_letter ?? null}
                            admin={admin === true} onAdminRelaunch={elevate}
                            onStaged={setStaged} staged={staged} />
              )}

            </div>
          </div>

          {/* Thanh điều hướng dính đáy vùng nội dung, không cuộn theo. Dùng lại
              lớp .shell nên hai nút thẳng hàng với hai mép của nội dung phía
              trên — trên màn rộng, "Tiếp tục" nằm ngay cạnh thứ vừa đọc xong
              chứ không bị đẩy ra góc màn hình. */}
          <div className="navbar">
            <div className="shell navbar__inner">
              <button className="btn" disabled={!prev} onClick={() => prev && setStep(prev)}>
                ← Quay lại
              </button>
              <span className="navbar__pos">
                Bước {at + 1}/{FLOW.length} · {meta(step, family).title}
              </span>
              <div className="spacer" />
              {next && (
                <button className="btn btn--primary" disabled={!unlocked(next)}
                        onClick={() => setStep(next)}>
                  Tiếp tục →
                </button>
              )}
            </div>
          </div>
        </main>
      </div>
    </div>
  );
}
