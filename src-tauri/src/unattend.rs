//! Sinh file `autounattend.xml` — bộ trả lời tự động cho trình cài đặt Windows.
//!
//! Windows Setup tự tìm file tên `autounattend.xml` ở **thư mục gốc của thiết bị
//! rời** khi khởi động. Có nó thì toàn bộ chuỗi màn hình hỏi đáp lúc đầu (vùng,
//! bàn phím, mạng, tài khoản, quyền riêng tư) được trả lời sẵn — đây chính là
//! phần mất nhiều thời gian nhất sau khi cài xong.
//!
//! File này chủ ý **không** cấu hình phân vùng đĩa. Thêm `DiskConfiguration` vào
//! sẽ khiến Setup tự xoá và chia lại ổ cứng đích mà không hỏi lại lần nào — quá
//! nguy hiểm cho một công cụ mà người dùng có thể cắm nhầm máy. Người dùng vẫn
//! tự chọn ổ và phân vùng như bình thường; thứ được bỏ qua chỉ là các màn hình
//! hỏi đáp.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAccount {
    pub name: String,
    pub password: String,
    /// Tự đăng nhập lần đầu, khỏi phải gõ mật khẩu ngay sau khi cài.
    pub auto_logon: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnattendConfig {
    pub enabled: bool,
    /// Ngôn ngữ **hiển thị** của Windows, vd `en-US`.
    ///
    /// Bị giới hạn bởi những gì nằm trong file ISO: đặt một ngôn ngữ không có
    /// trong ảnh đĩa thì Setup bỏ qua hoặc dừng giữa pass. Vì Microsoft không
    /// phát hành ISO tiếng Việt nên trường này không bao giờ là `vi-VN` —
    /// xem `languages.rs`.
    pub ui_language: String,
    /// Locale cho **định dạng vùng**: ngày tháng, tiền tệ, số. Locale nào cũng
    /// được, kể cả `vi-VN` trên một bản Windows tiếng Anh — và đó chính là thứ
    /// người dùng Việt Nam cần.
    pub locale: String,
    /// Mã bố cục bàn phím, vd `0409:00000409` (US).
    pub keyboard: String,
    /// Tên múi giờ theo Windows, vd `SE Asia Standard Time`.
    pub timezone: String,
    /// Để trống thì Windows tự đặt tên ngẫu nhiên.
    pub computer_name: String,
    pub local_account: Option<LocalAccount>,
    /// Bỏ qua các màn hình hỏi đáp và trang quảng cáo dịch vụ.
    pub skip_oobe: bool,
    /// Bỏ qua kiểm tra TPM / Secure Boot / RAM khi cài Windows 11.
    pub bypass_requirements: bool,
    /// `amd64` hoặc `arm64` — phải khớp với bộ cài, sai là Setup bỏ qua cả file.
    pub arch: String,
    /// Tên bản Windows trong `install.wim` sẽ được cài, vd `Windows 11 Pro`.
    ///
    /// Để trống nghĩa là **không chọn hộ** — Setup hiện bảng chọn như bình
    /// thường. Đây là mặc định, vì một ISO multi-edition có thể chứa tới mười
    /// bản và đoán hộ ở đó là đoán xem người dùng muốn cài bản nào.
    ///
    /// Giá trị phải khớp *nguyên văn* một mục trong `IsoInfo.editions`, vốn do
    /// `Get-WindowsImage` đọc thẳng từ chính file ISO đó — nên không có chuyện
    /// tên ở đây và tên trong ảnh đĩa lệch nhau.
    pub edition: String,
}

impl Default for UnattendConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            // Ngôn ngữ hiển thị phải là thứ tải được; định dạng vùng thì
            // mặc định theo người dùng Việt Nam.
            ui_language: "en-US".into(),
            locale: "vi-VN".into(),
            keyboard: "0409:00000409".into(),
            timezone: "SE Asia Standard Time".into(),
            computer_name: String::new(),
            local_account: None,
            skip_oobe: true,
            bypass_requirements: false,
            arch: "amd64".into(),
            // Không chọn hộ bản Windows: mặc định là Setup vẫn hỏi.
            edition: String::new(),
        }
    }
}

/// Thoát ký tự đặc biệt của XML.
///
/// Tên máy hay mật khẩu do người dùng gõ; một dấu `&` lọt vào là cả file hỏng
/// cú pháp và Setup báo "không đọc được answer file" — hỏng ở tận lúc cài, khi
/// người dùng đã đứng trước chiếc máy trống.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Thuộc tính chung của mọi thẻ `<component>`.
fn attrs(name: &str, arch: &str) -> String {
    format!(
        r#"name="{name}" processorArchitecture="{arch}" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance""#
    )
}

/// Các khoá registry mà Windows 11 Setup đọc để bỏ qua kiểm tra phần cứng.
const BYPASS_KEYS: &[&str] = &[
    "BypassTPMCheck",
    "BypassSecureBootCheck",
    "BypassRAMCheck",
    "BypassStorageCheck",
    "BypassCPUCheck",
];

pub fn generate(cfg: &UnattendConfig) -> Option<String> {
    if !cfg.enabled {
        return None;
    }

    let arch = if cfg.arch.eq_ignore_ascii_case("arm64") { "arm64" } else { "amd64" };
    let ui = esc(&cfg.ui_language);
    let loc = esc(&cfg.locale);
    let kbd = esc(&cfg.keyboard);

    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<unattend xmlns=\"urn:schemas-microsoft-com:unattend\">\r\n",
    );

    // ------------------------------------------------------- windowsPE
    xml.push_str("  <settings pass=\"windowsPE\">\r\n");
    xml.push_str(&format!(
        "    <component {}>\r\n\
         \x20     <SetupUILanguage><UILanguage>{ui}</UILanguage></SetupUILanguage>\r\n\
         \x20     <InputLocale>{kbd}</InputLocale>\r\n\
         \x20     <SystemLocale>{loc}</SystemLocale>\r\n\
         \x20     <UILanguage>{ui}</UILanguage>\r\n\
         \x20     <UserLocale>{loc}</UserLocale>\r\n\
         \x20   </component>\r\n",
        attrs("Microsoft-Windows-International-Core-WinPE", arch)
    ));

    xml.push_str(&format!("    <component {}>\r\n", attrs("Microsoft-Windows-Setup", arch)));

    // Chọn sẵn bản Windows nào trong install.wim sẽ được cài.
    //
    // `ImageInstall` gộp hai thứ khác hẳn nhau về mức nguy hiểm, và chỉ một
    // nửa của nó được sinh ra ở đây:
    //
    // - `InstallFrom` chọn **ảnh nào** trong install.wim. Sai thì cùng lắm là
    //   cài nhầm bản Home thay vì Pro — cài lại được.
    // - `InstallTo` chọn **ổ đích nào**. Có nó thì Setup ghi đè lên đúng phân
    //   vùng mà file này chỉ định, không hỏi lại lần nào. Đó chính là loại rủi
    //   ro mà `DiskConfiguration` mang lại và là lý do file này không có nó.
    //
    // Nên chỉ `InstallFrom` được sinh ra; người dùng vẫn tự chọn ổ và phân
    // vùng như bình thường. Có test khoá lại điều này.
    //
    // `WillShowUI=OnError` giữ đường lui: nếu tên không khớp ảnh nào trong ISO
    // — người dùng đổi sang file ISO khác chẳng hạn — Setup hiện lại bảng chọn
    // thay vì dừng giữa chừng với một lỗi mà không ai đọc được nguyên nhân.
    if !cfg.edition.trim().is_empty() {
        xml.push_str(&format!(
            "      <ImageInstall>\r\n\
             \x20       <OSImage>\r\n\
             \x20         <InstallFrom>\r\n\
             \x20           <MetaData wcm:action=\"add\">\r\n\
             \x20             <Key>/IMAGE/NAME</Key>\r\n\
             \x20             <Value>{}</Value>\r\n\
             \x20           </MetaData>\r\n\
             \x20         </InstallFrom>\r\n\
             \x20         <WillShowUI>OnError</WillShowUI>\r\n\
             \x20       </OSImage>\r\n\
             \x20     </ImageInstall>\r\n",
            esc(cfg.edition.trim())
        ));
    }

    xml.push_str("      <UserData><AcceptEula>true</AcceptEula></UserData>\r\n");

    if cfg.bypass_requirements {
        xml.push_str("      <RunSynchronous>\r\n");
        for (i, key) in BYPASS_KEYS.iter().enumerate() {
            xml.push_str(&format!(
                "        <RunSynchronousCommand wcm:action=\"add\">\r\n\
                 \x20         <Order>{}</Order>\r\n\
                 \x20         <Path>reg add \"HKLM\\SYSTEM\\Setup\\LabConfig\" /v {key} /t REG_DWORD /d 1 /f</Path>\r\n\
                 \x20       </RunSynchronousCommand>\r\n",
                i + 1
            ));
        }
        xml.push_str("      </RunSynchronous>\r\n");
    }
    xml.push_str("    </component>\r\n  </settings>\r\n");

    // ------------------------------------------------------- specialize
    xml.push_str("  <settings pass=\"specialize\">\r\n");
    xml.push_str(&format!("    <component {}>\r\n", attrs("Microsoft-Windows-Shell-Setup", arch)));
    if !cfg.computer_name.trim().is_empty() {
        xml.push_str(&format!("      <ComputerName>{}</ComputerName>\r\n", esc(cfg.computer_name.trim())));
    }
    xml.push_str(&format!("      <TimeZone>{}</TimeZone>\r\n", esc(&cfg.timezone)));
    xml.push_str("    </component>\r\n");

    if cfg.skip_oobe {
        // Từ Windows 11 24H2, Microsoft bỏ lệnh BypassNRO.cmd khỏi bộ cài. Khoá
        // registry thì vẫn còn tác dụng, và phải đặt ở pass specialize vì nó
        // chạy trước khi OOBE khởi động.
        xml.push_str(&format!(
            "    <component {}>\r\n\
             \x20     <RunSynchronous>\r\n\
             \x20       <RunSynchronousCommand wcm:action=\"add\">\r\n\
             \x20         <Order>1</Order>\r\n\
             \x20         <Path>reg add \"HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\OOBE\" /v BypassNRO /t REG_DWORD /d 1 /f</Path>\r\n\
             \x20       </RunSynchronousCommand>\r\n\
             \x20     </RunSynchronous>\r\n\
             \x20   </component>\r\n",
            attrs("Microsoft-Windows-Deployment", arch)
        ));
    }
    xml.push_str("  </settings>\r\n");

    // ------------------------------------------------------- oobeSystem
    xml.push_str("  <settings pass=\"oobeSystem\">\r\n");
    xml.push_str(&format!(
        "    <component {}>\r\n\
         \x20     <InputLocale>{kbd}</InputLocale>\r\n\
         \x20     <SystemLocale>{loc}</SystemLocale>\r\n\
         \x20     <UILanguage>{ui}</UILanguage>\r\n\
         \x20     <UserLocale>{loc}</UserLocale>\r\n\
         \x20   </component>\r\n",
        attrs("Microsoft-Windows-International-Core", arch)
    ));

    xml.push_str(&format!("    <component {}>\r\n", attrs("Microsoft-Windows-Shell-Setup", arch)));

    if cfg.skip_oobe {
        xml.push_str(
            "      <OOBE>\r\n\
             \x20       <HideEULAPage>true</HideEULAPage>\r\n\
             \x20       <HideOEMRegistrationScreen>true</HideOEMRegistrationScreen>\r\n\
             \x20       <HideOnlineAccountScreens>true</HideOnlineAccountScreens>\r\n\
             \x20       <HideLocalAccountScreen>true</HideLocalAccountScreen>\r\n\
             \x20       <HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE>\r\n\
             \x20       <NetworkLocation>Home</NetworkLocation>\r\n\
             \x20       <ProtectYourPC>3</ProtectYourPC>\r\n\
             \x20     </OOBE>\r\n",
        );
    }

    if let Some(acc) = &cfg.local_account {
        let name = esc(acc.name.trim());
        xml.push_str("      <UserAccounts>\r\n        <LocalAccounts>\r\n");
        xml.push_str(&format!(
            "          <LocalAccount wcm:action=\"add\">\r\n\
             \x20           <Name>{name}</Name>\r\n\
             \x20           <DisplayName>{name}</DisplayName>\r\n\
             \x20           <Group>Administrators</Group>\r\n"
        ));
        if !acc.password.is_empty() {
            xml.push_str(&format!(
                "            <Password><Value>{}</Value><PlainText>true</PlainText></Password>\r\n",
                esc(&acc.password)
            ));
        }
        xml.push_str("          </LocalAccount>\r\n        </LocalAccounts>\r\n      </UserAccounts>\r\n");

        // Tự đăng nhập chỉ khả thi khi có mật khẩu; Windows từ chối AutoLogon
        // với mật khẩu rỗng.
        if acc.auto_logon && !acc.password.is_empty() {
            xml.push_str(&format!(
                "      <AutoLogon>\r\n\
                 \x20       <Username>{name}</Username>\r\n\
                 \x20       <Enabled>true</Enabled>\r\n\
                 \x20       <LogonCount>1</LogonCount>\r\n\
                 \x20       <Password><Value>{}</Value><PlainText>true</PlainText></Password>\r\n\
                 \x20     </AutoLogon>\r\n",
                esc(&acc.password)
            ));
        }
    }

    xml.push_str("    </component>\r\n  </settings>\r\n</unattend>\r\n");
    Some(xml)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> UnattendConfig {
        UnattendConfig {
            enabled: true,
            computer_name: "MAY-CUA-TOI".into(),
            local_account: Some(LocalAccount {
                name: "MrBeoHP".into(),
                password: "matkhau".into(),
                auto_logon: true,
            }),
            ..Default::default()
        }
    }

    /// Kiểm tra thô rằng mọi thẻ mở đều có thẻ đóng tương ứng, đúng thứ tự.
    /// Không thay được một bộ phân tích XML thật, nhưng đủ bắt lỗi quên đóng thẻ
    /// — lỗi mà người dùng chỉ phát hiện khi đã đứng trước máy cần cài.
    fn well_formed(xml: &str) -> bool {
        let mut stack: Vec<String> = Vec::new();
        let bytes: Vec<char> = xml.chars().collect();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != '<' {
                i += 1;
                continue;
            }
            let Some(end) = bytes[i..].iter().position(|c| *c == '>') else { return false };
            let tag: String = bytes[i + 1..i + end].iter().collect();
            i += end + 1;

            if tag.starts_with('?') || tag.starts_with('!') || tag.ends_with('/') {
                continue;
            }
            if let Some(name) = tag.strip_prefix('/') {
                match stack.pop() {
                    Some(open) if open == name => {}
                    _ => return false,
                }
            } else {
                let name = tag.split_whitespace().next().unwrap_or("").to_string();
                stack.push(name);
            }
        }
        stack.is_empty()
    }

    #[test]
    fn disabled_config_produces_no_file() {
        let mut c = cfg();
        c.enabled = false;
        assert!(generate(&c).is_none());
    }

    #[test]
    fn output_is_well_formed_and_has_all_three_passes() {
        let xml = generate(&cfg()).unwrap();
        assert!(well_formed(&xml), "XML không cân thẻ:\n{xml}");
        for pass in ["windowsPE", "specialize", "oobeSystem"] {
            assert!(xml.contains(&format!("pass=\"{pass}\"")), "thiếu pass {pass}");
        }
    }

    #[test]
    fn special_characters_cannot_break_the_file() {
        // Mật khẩu có ký tự & < > là chuyện bình thường; không thoát thì cả file
        // hỏng cú pháp và Setup bỏ qua, người dùng phải bấm lại từ đầu.
        let mut c = cfg();
        c.computer_name = "MAY & CON".into();
        c.local_account = Some(LocalAccount {
            name: "a<b>c".into(),
            password: "p&w\"d".into(),
            auto_logon: false,
        });

        let xml = generate(&c).unwrap();
        assert!(well_formed(&xml), "XML không cân thẻ sau khi thoát ký tự");
        assert!(xml.contains("MAY &amp; CON"));
        assert!(xml.contains("a&lt;b&gt;c"));
        assert!(xml.contains("p&amp;w&quot;d"));
        assert!(!xml.contains("MAY & CON"));
    }

    #[test]
    fn oobe_screens_are_hidden_when_requested() {
        let xml = generate(&cfg()).unwrap();
        for tag in [
            "HideEULAPage",
            "HideOnlineAccountScreens",
            "HideWirelessSetupInOOBE",
            "ProtectYourPC",
        ] {
            assert!(xml.contains(tag), "thiếu {tag}");
        }
        assert!(xml.contains("BypassNRO"), "cần khoá này để tạo được tài khoản cục bộ trên bản mới");
    }

    #[test]
    fn hardware_bypass_is_opt_in() {
        let xml = generate(&cfg()).unwrap();
        assert!(!xml.contains("BypassTPMCheck"), "mặc định không được bỏ qua kiểm tra");

        let mut c = cfg();
        c.bypass_requirements = true;
        let xml = generate(&c).unwrap();
        assert!(well_formed(&xml));
        for k in BYPASS_KEYS {
            assert!(xml.contains(k), "thiếu khoá {k}");
        }
    }

    #[test]
    fn no_account_means_no_account_block() {
        let mut c = cfg();
        c.local_account = None;
        let xml = generate(&c).unwrap();
        assert!(well_formed(&xml));
        assert!(!xml.contains("<LocalAccounts>"));
        assert!(!xml.contains("<AutoLogon>"));
    }

    #[test]
    fn autologon_is_skipped_without_a_password() {
        // Windows từ chối tự đăng nhập với mật khẩu rỗng; sinh ra thẻ đó chỉ tạo
        // một lỗi khó hiểu ở lần khởi động đầu tiên.
        let mut c = cfg();
        c.local_account = Some(LocalAccount {
            name: "MrBeoHP".into(),
            password: String::new(),
            auto_logon: true,
        });
        let xml = generate(&c).unwrap();
        assert!(xml.contains("<LocalAccounts>"));
        assert!(!xml.contains("<AutoLogon>"));
        assert!(!xml.contains("<Password>"));
    }

    #[test]
    fn architecture_follows_the_iso() {
        let mut c = cfg();
        c.arch = "arm64".into();
        assert!(generate(&c).unwrap().contains("processorArchitecture=\"arm64\""));

        c.arch = "x64".into();
        assert!(
            generate(&c).unwrap().contains("processorArchitecture=\"amd64\""),
            "mọi giá trị không phải arm64 đều quy về amd64"
        );
    }

    /// Ngôn ngữ hiển thị và định dạng vùng là hai thứ khác nhau, và gộp chúng
    /// lại chính là lỗi khiến ứng dụng từng sinh ra `<UILanguage>vi-VN</...>`
    /// trên một bản ISO không hề có tiếng Việt.
    #[test]
    fn display_language_and_regional_format_are_kept_apart() {
        let cfg = UnattendConfig {
            enabled: true,
            ui_language: "en-US".into(),
            locale: "vi-VN".into(),
            ..Default::default()
        };
        let xml = generate(&cfg).expect("phải sinh ra file");

        assert!(xml.contains("<UILanguage>en-US</UILanguage>"),
            "ngôn ngữ hiển thị phải theo ISO");
        assert!(xml.contains("<UserLocale>vi-VN</UserLocale>"),
            "định dạng vùng vẫn phải là Việt Nam");
        assert!(xml.contains("<SystemLocale>vi-VN</SystemLocale>"));
        assert!(!xml.contains("<UILanguage>vi-VN</UILanguage>"),
            "không được đòi một ngôn ngữ hiển thị không có trong ảnh đĩa");
    }

    /// Cả hai pass đều phải nhất quán — sai một chỗ thì Setup và OOBE hiện hai
    /// ngôn ngữ khác nhau.
    #[test]
    fn both_passes_agree_on_the_display_language() {
        let cfg = UnattendConfig {
            enabled: true,
            ui_language: "ja-JP".into(),
            locale: "vi-VN".into(),
            ..Default::default()
        };
        let xml = generate(&cfg).unwrap();
        assert_eq!(xml.matches("<UILanguage>ja-JP</UILanguage>").count(), 3,
            "SetupUILanguage + windowsPE + oobeSystem");
        assert_eq!(xml.matches("<UserLocale>vi-VN</UserLocale>").count(), 2);
    }

    #[test]
    fn no_disk_configuration_is_ever_emitted() {
        // Hàng rào an toàn: có DiskConfiguration thì Setup tự xoá ổ cứng đích mà
        // không hỏi. Không bao giờ được sinh ra, kể cả khi thêm tính năng sau này.
        //
        // Chạy với cả hai trạng thái của bộ chọn phiên bản: khối `ImageInstall`
        // do nó sinh ra là chỗ gần `InstallTo` nhất trong cả file, nên đó chính
        // là chỗ một lần mở rộng sau này dễ vô tình kéo ổ đích vào nhất.
        for edition in ["", "Windows 11 Pro"] {
            let mut c = cfg();
            c.bypass_requirements = true;
            c.edition = edition.into();
            let xml = generate(&c).unwrap();
            assert!(!xml.contains("DiskConfiguration"), "edition = {edition:?}");
            assert!(!xml.contains("WillWipeDisk"), "edition = {edition:?}");
            // `InstallTo` là nửa nguy hiểm của ImageInstall: nó chỉ định phân
            // vùng đích, và có nó thì Setup ghi đè không hỏi lại.
            assert!(!xml.contains("InstallTo"), "edition = {edition:?}");
        }
    }

    /// Chọn phiên bản là chọn **ảnh nào trong install.wim**, và Setup đọc lựa
    /// chọn đó qua đúng một cặp khoá/giá trị.
    #[test]
    fn the_chosen_edition_becomes_an_image_name_metadata() {
        let mut c = cfg();
        c.edition = "Windows 11 Pro".into();
        let xml = generate(&c).unwrap();

        assert!(well_formed(&xml), "file phải đóng đủ thẻ");
        assert!(xml.contains("<Key>/IMAGE/NAME</Key>"));
        assert!(xml.contains("<Value>Windows 11 Pro</Value>"));
        // Khối phải nằm ở pass windowsPE — đó là pass duy nhất chạy trước khi
        // Setup hỏi cài bản nào. Đặt ở specialize thì câu hỏi đã trôi qua rồi.
        let pe = xml.split("pass=\"specialize\"").next().unwrap();
        assert!(pe.contains("<ImageInstall>"), "ImageInstall phải ở trong windowsPE");
    }

    /// Không chọn gì thì Setup vẫn hỏi. Sinh ra một `ImageInstall` rỗng sẽ
    /// khiến Setup dừng vì không biết cài ảnh nào — tệ hơn hẳn việc để nó hỏi.
    #[test]
    fn no_edition_means_setup_still_asks() {
        let xml = generate(&cfg()).unwrap();
        assert!(!xml.contains("ImageInstall"));
        assert!(!xml.contains("/IMAGE/NAME"));
    }

    /// Khoảng trắng thừa quanh tên là lỗi sao chép thường gặp, và Setup so tên
    /// nguyên văn nên `" Windows 11 Pro "` sẽ không khớp ảnh nào.
    #[test]
    fn the_edition_name_is_trimmed_and_escaped() {
        let mut c = cfg();
        c.edition = "  Windows 11 Pro  ".into();
        assert!(generate(&c).unwrap().contains("<Value>Windows 11 Pro</Value>"));

        // Tên bản do ISO khai, không phải hằng số trong mã nguồn — một dấu `&`
        // lọt vào là cả file hỏng cú pháp và Setup bỏ qua toàn bộ.
        c.edition = "Windows 10 Home & Pro".into();
        let xml = generate(&c).unwrap();
        assert!(xml.contains("<Value>Windows 10 Home &amp; Pro</Value>"));
        assert!(well_formed(&xml));
    }

    /// Chỉ chọn ảnh thì không kéo theo quyền gì thêm: bộ chọn phiên bản không
    /// được làm thay đổi phần còn lại của file.
    #[test]
    fn choosing_an_edition_changes_nothing_else() {
        let without = generate(&cfg()).unwrap();
        let mut c = cfg();
        c.edition = "Windows 11 Pro".into();
        let with = generate(&c).unwrap();

        let stripped = with
            .split_once("      <ImageInstall>")
            .and_then(|(head, rest)| rest.split_once("      </ImageInstall>\r\n").map(|(_, tail)| format!("{head}{tail}")))
            .expect("khối ImageInstall phải nằm gọn một chỗ");
        assert_eq!(stripped, without, "chỉ được thêm đúng khối ImageInstall");
    }
}
