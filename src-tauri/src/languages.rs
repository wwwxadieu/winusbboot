//! Ngôn ngữ của bộ cài Windows.
//!
//! Điểm cốt lõi của module này: **Microsoft không phát hành ISO Windows tiếng
//! Việt.** Trang tải chính thức không có mục nào cho tiếng Việt, và cũng chưa
//! từng có. Người dùng Việt Nam cài Windows tiếng Anh rồi thêm gói ngôn ngữ
//! hiển thị sau, hoặc đơn giản là dùng luôn tiếng Anh.
//!
//! Trước đây ứng dụng ghi cứng "Vietnamese" khi hỏi link tải và ghi cứng
//! "Tiếng Việt (vi-vn)" ở phần gợi ý — tức là hướng dẫn người dùng đi chọn một
//! thứ không tồn tại.
//!
//! Từ đó ra hai khái niệm phải tách bạch, vì gộp lại là nguồn gốc của cả lỗi
//! trên:
//!
//! - **Ngôn ngữ hiển thị** (`UILanguage`) bị giới hạn bởi những gì nằm trong
//!   file ISO. Chọn một ngôn ngữ không có trong ảnh đĩa thì Windows Setup bỏ
//!   qua, hoặc dừng ở giữa pass.
//! - **Định dạng vùng và bàn phím** (`SystemLocale`, `UserLocale`,
//!   `InputLocale`) thì locale nào cũng được, kể cả `vi-VN`. Đây chính là thứ
//!   người dùng Việt Nam cần trên một bản Windows tiếng Anh: ngày tháng, tiền
//!   tệ và bàn phím theo Việt Nam.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupLanguage {
    /// Đúng tên Microsoft dùng trên trang tải, để đối chiếu khi hỏi link.
    /// Rỗng nghĩa là Microsoft không phát hành ISO ở ngôn ngữ này.
    pub ms_name: String,
    /// Mã locale cho `autounattend.xml`.
    pub locale: String,
    /// Tên hiện trên giao diện.
    pub label: String,
    /// Bố cục bàn phím mặc định, dạng `LCID:KLID`.
    pub keyboard: String,
    /// `true` nghĩa là chỉ dùng được cho định dạng vùng và bàn phím, không có
    /// ISO tương ứng. Hiện chỉ có tiếng Việt rơi vào nhóm này.
    pub region_only: bool,
}

/// Gần như mọi bàn phím vật lý bán ra đều là bố cục US, và người dùng Việt Nam
/// gõ tiếng Việt bằng bộ gõ ngoài (UniKey, EVKey) chứ không đổi bố cục. Nên
/// mặc định để US thay vì suy ra bố cục từ locale — đoán sai bố cục thì màn
/// hình đăng nhập đầu tiên đã gõ không ra mật khẩu.
const US_KBD: &str = "0409:00000409";

fn l(ms_name: &str, locale: &str, label: &str) -> SetupLanguage {
    SetupLanguage {
        ms_name: ms_name.into(),
        locale: locale.into(),
        label: label.into(),
        keyboard: US_KBD.into(),
        region_only: ms_name.is_empty(),
    }
}

/// Ngôn ngữ mặc định. Có mặt trong mọi bản phát hành và là bản dễ tìm trợ giúp
/// nhất khi gặp lỗi.
pub const DEFAULT: &str = "English";

/// Toàn bộ bảng: ngôn ngữ có ISO, cộng những locale chỉ dùng cho vùng/bàn phím.
pub fn all() -> Vec<SetupLanguage> {
    vec![
        l("Arabic", "ar-SA", "Ả Rập"),
        l("Brazilian Portuguese", "pt-BR", "Bồ Đào Nha (Brazil)"),
        l("Bulgarian", "bg-BG", "Bulgaria"),
        l("Chinese (Simplified)", "zh-CN", "Trung (giản thể)"),
        l("Chinese (Traditional)", "zh-TW", "Trung (phồn thể)"),
        l("Croatian", "hr-HR", "Croatia"),
        l("Czech", "cs-CZ", "Séc"),
        l("Danish", "da-DK", "Đan Mạch"),
        l("Dutch", "nl-NL", "Hà Lan"),
        // Microsoft gọi bản này đúng một chữ "English" trong API tải, không phải
        // "English (United States)". Tên ở đây phải khớp từng ký tự với thứ API
        // trả về, vì khâu chọn SKU so bằng dấu bằng — so kiểu "bắt đầu bằng" sẽ
        // vớ nhầm sang "English International".
        l("English", "en-US", "Anh (Mỹ)"),
        l("English International", "en-GB", "Anh (quốc tế)"),
        l("Estonian", "et-EE", "Estonia"),
        l("Finnish", "fi-FI", "Phần Lan"),
        l("French", "fr-FR", "Pháp"),
        l("French Canadian", "fr-CA", "Pháp (Canada)"),
        l("German", "de-DE", "Đức"),
        l("Greek", "el-GR", "Hy Lạp"),
        l("Hebrew", "he-IL", "Do Thái"),
        l("Hungarian", "hu-HU", "Hungary"),
        l("Italian", "it-IT", "Ý"),
        l("Japanese", "ja-JP", "Nhật"),
        l("Korean", "ko-KR", "Hàn"),
        l("Latvian", "lv-LV", "Latvia"),
        l("Lithuanian", "lt-LT", "Lithuania"),
        l("Norwegian", "nb-NO", "Na Uy"),
        l("Polish", "pl-PL", "Ba Lan"),
        l("Portuguese", "pt-PT", "Bồ Đào Nha"),
        l("Romanian", "ro-RO", "Romania"),
        l("Russian", "ru-RU", "Nga"),
        l("Serbian Latin", "sr-Latn-RS", "Serbia (Latin)"),
        l("Slovak", "sk-SK", "Slovakia"),
        l("Slovenian", "sl-SI", "Slovenia"),
        l("Spanish", "es-ES", "Tây Ban Nha"),
        l("Spanish (Mexico)", "es-MX", "Tây Ban Nha (Mexico)"),
        l("Swedish", "sv-SE", "Thuỵ Điển"),
        l("Thai", "th-TH", "Thái"),
        l("Turkish", "tr-TR", "Thổ Nhĩ Kỳ"),
        l("Ukrainian", "uk-UA", "Ukraina"),
        // Không có ISO — chỉ dùng cho định dạng vùng và bàn phím.
        l("", "vi-VN", "Việt Nam"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lý do tồn tại của cả module này. Giao diện chào ra đúng những mục
    /// `region_only == false`, nên điều kiện phải khoá ở đây, trên bảng dữ liệu.
    #[test]
    fn vietnamese_is_never_offered_as_an_iso_language() {
        for x in all().into_iter().filter(|x| !x.region_only) {
            assert!(
                !x.locale.starts_with("vi"),
                "Microsoft không phát hành ISO tiếng Việt, nhưng {} đang được chào ra",
                x.label
            );
        }
    }

    /// …nhưng vẫn phải chọn được cho định dạng vùng và bàn phím, vì đó mới là
    /// thứ người dùng Việt Nam cần trên một bản Windows tiếng Anh.
    #[test]
    fn vietnamese_is_still_available_for_region_and_keyboard() {
        let vi = all().into_iter().find(|x| x.locale == "vi-VN").expect("thiếu vi-VN");
        assert!(vi.region_only);
        assert!(vi.ms_name.is_empty(), "region_only thì không được có tên ISO");
    }

    #[test]
    fn the_default_language_is_actually_in_the_list() {
        assert!(all().iter().any(|x| !x.region_only && x.ms_name == DEFAULT));
    }

    #[test]
    fn every_entry_is_well_formed_and_unique() {
        let all = all();
        let mut locales: Vec<&str> = all.iter().map(|x| x.locale.as_str()).collect();
        let before = locales.len();
        locales.sort_unstable();
        locales.dedup();
        assert_eq!(before, locales.len(), "locale bị trùng");

        for x in &all {
            assert!(!x.label.is_empty(), "{} thiếu nhãn hiển thị", x.locale);
            assert!(x.locale.contains('-'), "{} không phải mã locale hợp lệ", x.locale);
            assert!(!x.keyboard.is_empty(), "{} thiếu bố cục bàn phím", x.locale);
            // region_only và ms_name phải luôn nhất quán với nhau, nếu không thì
            // bộ lọc của giao diện sẽ chào ra một ngôn ngữ không tải được.
            assert_eq!(x.region_only, x.ms_name.is_empty(), "{} mâu thuẫn", x.locale);
        }
    }

    /// Giao diện tra locale bằng `ms_name`, so bằng đúng từng ký tự. Trùng tên
    /// giữa hai mục thì phép tra đó trả về mục nào là chuyện may rủi.
    #[test]
    fn iso_names_are_unique_so_the_locale_lookup_is_unambiguous() {
        let all = all();
        let mut names: Vec<&str> =
            all.iter().filter(|x| !x.region_only).map(|x| x.ms_name.as_str()).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(before, names.len(), "tên ISO bị trùng");
    }
}
