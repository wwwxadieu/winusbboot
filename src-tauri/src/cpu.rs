//! Đánh giá CPU có nằm trong danh sách hỗ trợ chính thức của Windows 11 hay không.
//!
//! Microsoft công bố danh sách tường minh gồm hàng nghìn mã CPU. Nhúng trọn danh
//! sách đó vào app sẽ phình dung lượng và lạc hậu ngay khi có dòng chip mới, nên
//! ở đây dùng quy tắc theo *thế hệ* — bao phủ gần hết máy phổ thông và luôn nói rõ
//! mức độ chắc chắn để người dùng không bị hiểu nhầm là kết luận tuyệt đối.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuSupport {
    /// Gần như chắc chắn nằm trong danh sách chính thức.
    Supported,
    /// Gần như chắc chắn KHÔNG nằm trong danh sách.
    Unsupported,
    /// Không suy ra được — cần tra cứu thủ công.
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuVerdict {
    pub vendor: String,
    pub family: String,
    pub generation: Option<String>,
    pub support: CpuSupport,
    pub reason: String,
}

/// Lấy cụm số đứng ngay sau `marker`, kèm theo số chữ số đọc được.
///
/// Quét mọi vị trí xuất hiện của `marker` chứ không chỉ vị trí đầu: tên như
/// "Pentium Gold G6400" có tới hai chỗ khớp " g" và chỉ chỗ thứ hai mới có số.
fn number_after(hay: &str, marker: &str) -> Option<(u32, usize)> {
    let mut from = 0usize;
    while let Some(rel) = hay[from..].find(marker) {
        let start = from + rel + marker.len();
        let digits: String = hay[start..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            if let Ok(v) = digits.parse::<u32>() {
                return Some((v, digits.len()));
            }
        }
        from = start.max(from + rel + 1);
    }
    None
}

/// Suy ra thế hệ Intel Core từ mã sản phẩm.
///
/// Intel dùng hai quy ước chồng lên nhau: mã 4 chữ số bắt đầu bằng 1 là đời 10
/// trở lên (i5-1135G7 → đời 11), còn lại thì chữ số đầu chính là đời (i7-8550U
/// → đời 8). Nhầm hai quy ước này là lỗi kinh điển khi tự viết bộ kiểm tra.
fn intel_generation(num: u32, len: usize) -> u32 {
    match len {
        0..=3 => 1,
        4 if (1000..2000).contains(&num) => num / 100,
        4 => num / 1000,
        _ => num / 1000,
    }
}

/// Cụm số dài ít nhất `min_len` chữ số đầu tiên gặp trong chuỗi.
fn first_number(hay: &str, min_len: usize) -> Option<(u32, usize)> {
    let bytes: Vec<char> = hay.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let len = j - i;
            if len >= min_len {
                let s: String = bytes[i..j].iter().collect();
                if let Ok(v) = s.parse::<u32>() {
                    return Some((v, len));
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

/// Vài mã CPU đời 7 vẫn được Microsoft hỗ trợ (chủ yếu do có mặt trong dòng Surface).
const INTEL_GEN7_EXCEPTIONS: &[&str] = &["7820hq", "7920hq", "7700k"];

pub fn analyze(raw_name: &str) -> CpuVerdict {
    let name = raw_name.to_lowercase();

    if name.contains("qualcomm") || name.contains("snapdragon") || name.contains("microsoft sq") {
        return CpuVerdict {
            vendor: "Qualcomm".into(),
            family: "Snapdragon".into(),
            generation: None,
            support: CpuSupport::Supported,
            reason: "Chip ARM của Qualcomm — chạy bản Windows 11 ARM64.".into(),
        };
    }

    // AMD phải được xét trước Intel: rất nhiều tên CPU AMD chứa chữ "Core"
    // (vd: "AMD Ryzen 5 3600 6-Core Processor") và sẽ bị nhận nhầm nếu chỉ dựa
    // vào từ khoá đó để đoán hãng.
    if name.contains("amd")
        || name.contains("ryzen")
        || name.contains("athlon")
        || name.contains("epyc")
        || name.contains("threadripper")
    {
        return analyze_amd(&name);
    }
    if name.contains("intel")
        || name.contains("core i")
        || name.contains("core(tm) i")
        || name.contains("core ultra")
        || name.contains("xeon")
        || name.contains("pentium")
        || name.contains("celeron")
    {
        return analyze_intel(&name);
    }

    CpuVerdict {
        vendor: "Không rõ".into(),
        family: raw_name.to_string(),
        generation: None,
        support: CpuSupport::Unknown,
        reason: "Không nhận diện được dòng CPU. Hãy tra cứu danh sách CPU hỗ trợ của Microsoft.".into(),
    }
}

fn analyze_intel(name: &str) -> CpuVerdict {
    let mk = |family: &str, gen: Option<String>, support: CpuSupport, reason: String| CpuVerdict {
        vendor: "Intel".into(),
        family: family.into(),
        generation: gen,
        support,
        reason,
    };

    // Core Ultra (Meteor Lake trở đi) — luôn được hỗ trợ.
    if name.contains("core ultra") || name.contains("core(tm) ultra") {
        return mk(
            "Intel Core Ultra",
            Some("Ultra".into()),
            CpuSupport::Supported,
            "Dòng Core Ultra thế hệ mới, đáp ứng đầy đủ yêu cầu Windows 11.".into(),
        );
    }

    // Core i3/i5/i7/i9 — thế hệ nằm ở phần nghìn của mã sản phẩm.
    for tag in ["i3", "i5", "i7", "i9"] {
        let found = number_after(name, &format!("{tag}-"))
            .or_else(|| number_after(name, &format!("{tag} ")));
        let Some((num, len)) = found else { continue };

        let family = format!("Intel Core {}", tag);
        let gen = intel_generation(num, len);

        let is_exception = INTEL_GEN7_EXCEPTIONS.iter().any(|e| name.contains(e));
        return if gen >= 8 {
            mk(&family, Some(format!("đời {gen}")), CpuSupport::Supported,
               format!("Intel Core đời {gen} — nằm trong danh sách hỗ trợ Windows 11 (từ đời 8 trở lên)."))
        } else if is_exception {
            mk(&family, Some(format!("đời {gen}")), CpuSupport::Supported,
               "Đây là một trong số ít CPU đời 7 được Microsoft bổ sung vào danh sách hỗ trợ.".into())
        } else {
            mk(&family, Some(format!("đời {gen}")), CpuSupport::Unsupported,
               format!("Intel Core đời {gen} nằm dưới mốc đời 8 mà Windows 11 yêu cầu."))
        };
    }

    if name.contains("xeon") {
        let gen = first_number(name, 4).map(|(n, _)| n.to_string());
        return mk(
            "Intel Xeon",
            gen,
            CpuSupport::Unknown,
            "Dòng Xeon có rất nhiều nhánh; hãy đối chiếu trực tiếp với danh sách CPU hỗ trợ của Microsoft.".into(),
        );
    }

    // Celeron / Pentium / Atom và cả những chip chỉ ghi trần mã "N100".
    {
        let family = if name.contains("celeron") {
            "Intel Celeron"
        } else if name.contains("pentium") {
            "Intel Pentium"
        } else if name.contains("atom") {
            "Intel Atom"
        } else {
            "Intel"
        };

        for prefix in ["n", "j", "x", "g"] {
            // Mã luôn đứng sau dấu cách để tránh bắt nhầm chữ cái trong tên.
            let Some((num, len)) = number_after(name, &format!(" {prefix}")) else { continue };
            let label = format!("{}{}", prefix.to_uppercase(), num);

            let support = match (prefix, len) {
                // N100/N200/N305 — Alder Lake-N, ra mắt sau Windows 11.
                ("n", 3) => CpuSupport::Supported,
                // Gemini Lake (N4000) trở đi được hỗ trợ; Apollo Lake (N3350) thì không.
                ("n" | "j", 4) if num >= 4000 => CpuSupport::Supported,
                ("n" | "j", 4) => CpuSupport::Unsupported,
                // Atom x6000 (Elkhart Lake).
                ("x", 4) if num >= 6000 => CpuSupport::Supported,
                // Pentium Gold G5400 (Coffee Lake) trở lên.
                ("g", 4) if num >= 5000 => CpuSupport::Supported,
                ("g", 4) => CpuSupport::Unsupported,
                _ => continue,
            };

            let reason = match support {
                CpuSupport::Supported => format!("{family} {label} thuộc nhóm nhân mà Windows 11 hỗ trợ."),
                CpuSupport::Unsupported => format!("{family} {label} thuộc thế hệ ra trước mốc hỗ trợ của Windows 11."),
                CpuSupport::Unknown => format!("Không xác định được thế hệ của {family} {label}."),
            };
            return mk(family, Some(label), support, reason);
        }
    }

    mk(
        "Intel",
        None,
        CpuSupport::Unknown,
        "Không suy ra được thế hệ từ tên CPU.".into(),
    )
}

fn analyze_amd(name: &str) -> CpuVerdict {
    let mk = |family: &str, gen: Option<String>, support: CpuSupport, reason: String| CpuVerdict {
        vendor: "AMD".into(),
        family: family.into(),
        generation: gen,
        support,
        reason,
    };

    if name.contains("threadripper") {
        let (num, len) = first_number(name, 4).unwrap_or((0, 0));
        let series = if len == 4 { num / 1000 } else { 0 };
        return if series >= 2 {
            mk("AMD Threadripper", Some(format!("series {series}000")), CpuSupport::Supported,
               "Threadripper từ series 2000 trở lên nằm trong danh sách hỗ trợ.".into())
        } else {
            mk("AMD Threadripper", Some("series 1000".into()), CpuSupport::Unsupported,
               "Threadripper series 1000 không nằm trong danh sách hỗ trợ Windows 11.".into())
        };
    }

    if name.contains("ryzen") {
        // "Ryzen AI 9 HX 370" dùng mã 3 chữ số, các dòng còn lại dùng 4 chữ số.
        let ai_series = name.contains("ryzen ai");
        let tier = ["3", "5", "7", "9"]
            .iter()
            .find(|t| name.contains(&format!("ryzen {t}")))
            .map(|t| format!("Ryzen {t}"))
            .unwrap_or_else(|| "Ryzen".into());
        let family = format!("AMD {tier}");

        if ai_series {
            return mk(&family, Some("Ryzen AI".into()), CpuSupport::Supported,
                      "Dòng Ryzen AI ra mắt sau Windows 11, được hỗ trợ đầy đủ.".into());
        }

        let Some((num, len)) = first_number(name, 3) else {
            return mk(&family, None, CpuSupport::Unknown, "Không đọc được mã sản phẩm.".into());
        };
        if len == 3 {
            return mk(&family, Some(num.to_string()), CpuSupport::Supported,
                      "Mã 3 chữ số thuộc các dòng Ryzen thế hệ mới, được hỗ trợ.".into());
        }
        let series = num / 1000;
        return if series >= 2 {
            mk(&family, Some(format!("series {series}000")), CpuSupport::Supported,
               format!("Ryzen series {series}000 nằm trong danh sách hỗ trợ Windows 11 (từ series 2000 trở lên)."))
        } else {
            mk(&family, Some("series 1000".into()), CpuSupport::Unsupported,
               "Ryzen series 1000 (Zen đời đầu) không nằm trong danh sách hỗ trợ Windows 11.".into())
        };
    }

    if name.contains("athlon") {
        let ok = name.contains("gold") || name.contains("silver")
            || first_number(name, 4).map(|(n, _)| n >= 3000).unwrap_or(false);
        return if ok {
            mk("AMD Athlon", None, CpuSupport::Supported,
               "Athlon Gold/Silver và series 3000 nằm trong danh sách hỗ trợ.".into())
        } else {
            mk("AMD Athlon", None, CpuSupport::Unsupported,
               "Athlon đời cũ không đáp ứng yêu cầu CPU của Windows 11.".into())
        };
    }

    if name.contains("epyc") {
        return mk("AMD EPYC", None, CpuSupport::Unknown,
                  "Dòng máy chủ EPYC — hãy đối chiếu danh sách chính thức của Microsoft.".into());
    }

    // FX, A-series, Phenom: kiến trúc trước Zen, chắc chắn không được hỗ trợ.
    if name.contains("fx-") || name.contains("phenom") || name.contains("a10-")
        || name.contains("a8-") || name.contains("a6-") || name.contains("e1-")
    {
        return mk("AMD (trước Zen)", None, CpuSupport::Unsupported,
                  "Kiến trúc ra trước Zen, không nằm trong danh sách hỗ trợ Windows 11.".into());
    }

    mk("AMD", None, CpuSupport::Unknown, "Không suy ra được thế hệ từ tên CPU.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(n: &str) -> CpuSupport {
        analyze(n).support
    }

    #[test]
    fn intel_core_generations() {
        assert_eq!(s("11th Gen Intel(R) Core(TM) i5-1135G7 @ 2.40GHz"), CpuSupport::Supported);
        assert_eq!(s("Intel(R) Core(TM) i7-8550U CPU @ 1.80GHz"), CpuSupport::Supported);
        assert_eq!(s("Intel(R) Core(TM) i5-7200U CPU @ 2.50GHz"), CpuSupport::Unsupported);
        assert_eq!(s("Intel(R) Core(TM) i5-4200U CPU @ 1.60GHz"), CpuSupport::Unsupported);
        assert_eq!(s("Intel(R) Core(TM) i7-920 @ 2.67GHz"), CpuSupport::Unsupported);
        assert_eq!(s("Intel(R) Core(TM) i9-14900K"), CpuSupport::Supported);
        assert_eq!(s("Intel(R) Core(TM) Ultra 7 155H"), CpuSupport::Supported);
        // Mã 4 chữ số bắt đầu bằng 1 phải ra đời 10+, không phải đời 1.
        let v = analyze("Intel(R) Core(TM) i7-1065G7 CPU @ 1.30GHz");
        assert_eq!(v.support, CpuSupport::Supported);
        assert_eq!(v.generation.as_deref(), Some("đời 10"));
        assert_eq!(analyze("11th Gen Intel(R) Core(TM) i5-1135G7 @ 2.40GHz").generation.as_deref(), Some("đời 11"));
    }

    #[test]
    fn intel_gen7_exception_is_honoured() {
        assert_eq!(s("Intel(R) Core(TM) i7-7820HQ CPU @ 2.90GHz"), CpuSupport::Supported);
    }

    #[test]
    fn intel_low_power_lines() {
        assert_eq!(s("Intel(R) Celeron(R) N4020 CPU @ 1.10GHz"), CpuSupport::Supported);
        assert_eq!(s("Intel(R) Celeron(R) N3350 CPU @ 1.10GHz"), CpuSupport::Unsupported);
        assert_eq!(s("Intel(R) N100"), CpuSupport::Supported);
    }

    #[test]
    fn amd_ryzen_series() {
        assert_eq!(s("AMD Ryzen 5 3600 6-Core Processor"), CpuSupport::Supported);
        assert_eq!(s("AMD Ryzen 5 2600 Six-Core Processor"), CpuSupport::Supported);
        assert_eq!(s("AMD Ryzen 5 1600 Six-Core Processor"), CpuSupport::Unsupported);
        assert_eq!(s("AMD Ryzen 7 7840HS with Radeon Graphics"), CpuSupport::Supported);
        assert_eq!(s("AMD Ryzen AI 9 HX 370 w/ Radeon 890M"), CpuSupport::Supported);
    }

    #[test]
    fn amd_pre_zen_is_rejected() {
        assert_eq!(s("AMD FX-8350 Eight-Core Processor"), CpuSupport::Unsupported);
        assert_eq!(s("AMD A10-7860K APU"), CpuSupport::Unsupported);
    }

    #[test]
    fn amd_names_containing_the_word_core_are_not_read_as_intel() {
        for n in [
            "AMD Ryzen 5 3600 6-Core Processor",
            "AMD FX-8350 Eight-Core Processor",
            "AMD Ryzen 9 5950X 16-Core Processor",
        ] {
            assert_eq!(analyze(n).vendor, "AMD", "nhận nhầm hãng cho: {n}");
        }
    }

    #[test]
    fn arm_is_supported() {
        assert_eq!(s("Snapdragon(R) X Elite - X1E80100"), CpuSupport::Supported);
    }
}
