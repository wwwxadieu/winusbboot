# Get WinUSB

Ứng dụng Windows tự nhận diện ổ USB đang cắm, quét cấu hình máy, gợi ý hệ điều hành
phù hợp nhất — Windows hoặc một trong chín bản Linux — rồi tạo luôn USB cài đặt.

Tauri 2 + React 18 + TypeScript. Backend Rust, giao diện kính xếp lớp có chế độ sáng/tối.

---

## Ứng dụng làm gì

Sáu bước, chung cho cả hai họ hệ điều hành:

> **Hệ điều hành → Ổ USB → Phiên bản → Bộ cài → Ghi → Xong**

**1. Chọn hệ điều hành** — bước đầu tiên, và nó quyết định hình dạng của mọi bước sau.
Hai họ hệ điều hành được tạo USB theo hai cách khác hẳn nhau, không phải một cách có
tham số. Xem "Vì sao ISO Linux phải ghi nguyên khối" bên dưới. Chọn xong là sang bước sau
luôn — câu hỏi có đúng hai đáp án thì không cần bấm thêm "Tiếp tục".

| | Windows | Linux |
|---|---|---|
| Cách ghi | Format rồi chép file lên | Ghi nguyên khối, tương đương `dd` |
| Cài đặt tự động | `autounattend.xml` | Không có |
| Ổ USB tối thiểu | 8 GB | Vừa đúng cỡ file ISO |
| Kiểm tra sau khi ghi | Đối chiếu từng file với ISO | Băm lại từng byte đã ghi |
| Kèm driver | Có, qua `$WinPEDriver$` | Không cần — driver nằm trong nhân |

**2. Chọn ổ USB** — một tiến trình PowerShell chạy nền suốt vòng đời ứng dụng, cứ 3 giây
in danh sách ổ USB dưới dạng JSON. Rust đọc từng dòng và chỉ đẩy sự kiện lên giao diện khi
danh sách thực sự đổi. Cắm hay rút ổ thì danh sách tự cập nhật, không cần bấm gì. Cắm đúng
một ổ dùng được thì ứng dụng chọn sẵn hộ; từ hai ổ trở lên thì không, vì đoán hộ ở đó là
đoán xem ổ nào bị xoá.

**3. Chọn phiên bản** — đối chiếu cấu hình máy với yêu cầu của từng bản Windows rồi chấm
điểm 0–100. Mỗi bản được gắn một trong bốn kết luận:

| Kết luận | Ý nghĩa |
|---|---|
| Cài được ngay | Đủ mọi điều kiện |
| Cần chỉnh BIOS | Phần cứng có sẵn, chỉ đang tắt (TPM, Secure Boot) |
| Phải bỏ qua kiểm tra | Thiếu điều kiện cứng nhưng vẫn cài được nếu chấp nhận rủi ro |
| Không cài được | Thiếu điều kiện không thể lách (RAM, dung lượng, kiến trúc) |

Điểm mấu chốt là engine phân biệt **"máy không có TPM"** với **"máy có TPM nhưng đang tắt"** —
trường hợp thứ hai chỉ cần vào BIOS bật lên, không cần lách gì cả. Với máy thật sự không có
TPM 2.0, ứng dụng đề xuất Windows 10 IoT Enterprise LTSC 2021: vẫn còn bản vá bảo mật
tới 13/01/2032, an toàn hơn nhiều so với việc cài Windows 11 lách kiểm tra.

Ngay dưới bảng gợi ý là khối **Chi tiết phần cứng**, gập lại sẵn: 13 mục đối chiếu với yêu
cầu chính thức của Windows 11 — bộ xử lý (kiểu, số nhân, xung nhịp, kiến trúc), RAM, dung
lượng ổ, chế độ khởi động, Secure Boot, TPM, card đồ hoạ, và ba mục màn hình. Đây từng là
một bước riêng, nhưng người dùng không *làm* gì ở đó, chỉ đọc — mà bắt bấm "Tiếp tục" để đi
qua một trang chưa chắc đã muốn xem thì đó là một bước thừa.

Bốn trạng thái, và ranh giới giữa chúng mới là phần quan trọng:

| Dấu | Nghĩa |
|---|---|
| Xanh | Đạt |
| Vàng | Chưa đạt nhưng chỉ cần chỉnh thiết lập, không phải thay phần cứng |
| Đỏ | Không đạt |
| Xám | Không đọc được — **không phải là không đạt** |

Gộp vàng vào đỏ sẽ đẩy một chiếc máy chỉ cần bật TPM trong BIOS sang cài Windows 10.
Gộp xám vào đỏ thì tệ hơn: đó là báo hỏng cho thứ mà ứng dụng chưa biết. Cả hai ranh giới
này đều có test khoá lại.

> **Đọc TPM và Secure Boot mà không cần quyền quản trị.** `Win32_Tpm` nằm trong namespace
> bảo mật và `Confirm-SecureBootUEFI` đều đòi quyền Administrator — chạy ở quyền thường thì
> cả hai ném Access Denied, khiến ứng dụng tưởng máy không có TPM và gợi ý sai hẳn phiên bản.
> Ứng dụng vì thế có đường đọc thay thế: TPM lấy từ Device Manager (tên thiết bị chứa sẵn
> phiên bản, vd "Trusted Platform Module 2.0"), Secure Boot lấy từ khoá registry
> `HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\State`. Cả hai đều đọc được bằng tài
> khoản thường và đủ chính xác để kết luận; giao diện ghi rõ thông tin đến từ nguồn nào.

**4. Bộ cài** — chọn file ISO có sẵn, tải tự động từ nguồn chính thức (cả Windows lẫn
Linux, nối tiếp được khi đứt mạng), hoặc mở trang tải trong trình duyệt. Đọc luôn nội dung ISO:
có những bản Windows nào, kiến trúc gì, install.wim nặng bao nhiêu. Có nút tính SHA-256; riêng
bản Linux thì đối chiếu tự động với mã băm dự án công bố.

**5. Ghi** — một nút, tám chặng đếm liền một mạch: xoá ổ và chia lại phân vùng (2 chặng),
rồi chép file, tách install.wim nếu cần, ghi mã khởi động, ghi `autounattend.xml` (6 chặng).
Tiến trình hiện theo byte thực tế, kể cả khi đang chép một file 5 GB đơn lẻ. Luồng Linux thì
ghi nguyên khối, ba chặng.

Format từng là một bước riêng, đặt trước bước ghi. Tách ra không bảo vệ được gì thêm: không
có luồng nào format xong rồi dừng lại, nên nó chỉ thêm một trang phải đọc, một ô phải tick,
một nút phải bấm — cộng một trạng thái hỏng người dùng tự tạo ra được, là format ổ này rồi
đi ghi lên ổ khác. Ô xác nhận xoá dữ liệu thì vẫn còn nguyên, chỉ còn đúng một cái.

Kiểu phân vùng (GPT+FAT32, MBR+FAT32, MBR+NTFS) và tên ổ nằm trong khối **Kiểu phân vùng và
tên ổ**, gập lại sẵn: mặc định đúng cho gần như mọi máy, và ISO không boot được UEFI thì ứng
dụng tự chuyển sang MBR.

**6. Xong** — mọi việc *sau khi* đã ghi. Phần kiểm tra khởi động chạy ngay khi mở bước này
và hiện kết luận (xem "Vì sao cần bước kiểm tra" bên dưới); phần đối chiếu lại từng byte và
phần **Kèm driver vào USB** thì gập lại, vì không phải ai cũng cần.

Kèm driver *(chỉ luồng Windows)* — xuất driver của chính máy đang chạy, hoặc chọn một thư
mục driver có sẵn, rồi chép vào USB để Windows Setup tự cài trong lúc cài máy. Ứng dụng đối
chiếu từng card mạng và ổ đĩa của máy với mã phần cứng khai trong các file `.inf` để nói
thẳng: card Wi-Fi này **đã có** driver trong bộ sắp chép, hay chưa. Xem "Kèm driver để cài
xong là có Wi-Fi" bên dưới.

### Vì sao cần bước kiểm tra

Ghi xong không có nghĩa là boot được, và không nguyên nhân nào dưới đây báo lỗi lúc ghi:

1. **Chép hụt.** Ổ đầy giữa chừng, hoặc một file bị khoá nên bị bỏ qua — `robocopy` vẫn
   kết thúc với mã thành công.
2. **Thiếu đường khởi động.** Đủ file cài đặt nhưng thiếu `bootmgr` hoặc
   `efi\boot\bootx64.efi`, nên firmware không tìm ra thứ gì để chạy.
3. **USB dối.** Ổ khai 128 GB nhưng thật ra chỉ có 8 GB, hoặc flash đã gần chết. Ghi thì
   "thành công" vì thiết bị nhận hết dữ liệu rồi vứt đi.

Nhóm 1 và 2 phát hiện được bằng cách đọc cấu trúc ổ — vài giây, chạy ngay khi mở bước này.
Nhóm 3 thì mọi kiểm tra cấu trúc đều báo xanh; chỉ có **đọc ngược toàn bộ dữ liệu vừa ghi
và đối chiếu** mới lộ ra, và việc đó mất gần bằng thời gian ghi nên phải bấm mới chạy.

Luồng Windows đối chiếu SHA-256 từng file giữa ảnh ISO gắn tạm và bản trên USB. Luồng
Linux băm lại đúng số byte đã ghi trên `\\.\PHYSICALDRIVE<n>` rồi so với mã băm của file
ảnh — khớp nghĩa là giống nhau từng byte.

Ba ranh giới quan trọng, đều có test khoá:

- **"Không đọc được" không phải "không đạt".** Cùng nguyên tắc mà phần quét phần cứng đã
  theo: kết luận một chiếc USB tốt là hỏng sẽ khiến người dùng ghi lại một cách vô ích.
- **Thiếu một đường khởi động không phải là hỏng.** Ổ chỉ có mã UEFI vẫn chạy tốt trên máy
  đời mới; báo đỏ ở đó là sai.
- **"Không khởi động được" khác "khởi động được nhưng bộ cài hỏng".** Ổ thiếu `install.wim`
  vẫn boot vào Windows Setup rồi mới dừng. Gộp hai trường hợp làm một thì kết luận tự mâu
  thuẫn với chính bảng đường khởi động ngay bên dưới nó, và người dùng đi tìm nhầm nguyên
  nhân. Lỗi này đã xảy ra một lần và giờ có test riêng.

Một mục `Fail` không chặn — ví dụ thiếu `autounattend.xml` — không hạ kết luận xuống "không
khởi động được": nó làm hỏng trải nghiệm chứ không làm hỏng việc khởi động.

Toàn bộ phần *đánh giá* là hàm thuần, tách khỏi phần thu thập dữ liệu qua PowerShell, nên
kiểm thử được mà không cần máy Windows. 24 test cho riêng phần này.

### Gợi ý bản Linux

Cùng một `HardwareReport` đã quét cho Windows, chấm theo thước đo khác. Windows hỏi
"máy có TPM 2.0 và CPU nằm trong danh sách hỗ trợ không"; Linux thì gần như máy nào cũng
cài được, nên câu hỏi thật là **"bản nào chạy mượt trên đúng lượng RAM này"**.

Chín bản trong danh mục: Ubuntu 24.04 LTS, Linux Mint 22 (Cinnamon và XFCE), Debian 13,
Fedora Workstation 43, Pop!_OS 22.04 LTS, Zorin OS 17, Lubuntu 24.04 LTS, Arch Linux.

Ba ranh giới engine phải phân biệt cho đúng:

- **"Đủ RAM tối thiểu" khác "đủ RAM để dùng thoải mái".** Gộp hai mốc này lại thì máy
  4 GB sẽ được gợi ý GNOME, rồi người dùng kết luận "Linux chạy chậm". Tính theo tỉ lệ
  chứ không theo bậc: máy 7,5 GB và máy 4 GB đều "dưới mức khuyến nghị 8 GB", nhưng trải
  nghiệm của hai máy đó khác nhau rất xa.
- **Secure Boot đang bật là một thiết lập BIOS, không phải rào chặn.** Bản không có shim
  ký sẵn (Arch) chỉ bị trừ điểm khi máy *đang bật* Secure Boot. Máy chạy BIOS cũ đọc ra
  `None` — coi `None` như "đang bật" sẽ cảnh báo sai cho mọi máy đời cũ.
- **"Desktop nhẹ" khác "không có desktop nào cả".** Arch được xếp `Light` vì nó không có
  desktop, không phải vì desktop của nó nhẹ. Thưởng điểm "desktop nhẹ" cho nó sẽ đẩy đúng
  bản khó cài nhất lên đầu bảng cho đúng nhóm máy của người dùng ít kinh nghiệm nhất —
  lỗi này đã xảy ra một lần và giờ có test khoá lại.

### Luồng tải ISO Windows: hỏng, bỏ, rồi dựng lại

Microsoft gỡ endpoint `/api/controls/contentinclude/html` mà luồng lấy link cũ dựa vào — nó trả
404 với mọi `pageId`. Tính năng tải tự động cho Windows vì thế đã bị bỏ hẳn một thời gian: giữ
lại một nút bấm chỉ dẫn tới lỗi thì tệ hơn là không có nút.

Sau đó Microsoft dựng lại luồng mới ở `software-download-connector/api`, và tính năng được viết
lại theo đúng hình dạng đó. Hình dạng này **đọc thẳng từ JavaScript của trang tải** chứ không
đoán, nên khớp từng tham số với thứ trình duyệt thật gửi đi:

```
# 1. Ghi nhận phiên
GET vlscppe.microsoft.com/tags?org_id=y6jn8c31&session_id=<GUID>

# 2. Nhận thử thách chống bot
GET ov-df.microsoft.com/mdt.js?instanceId=<hằng số>&PageId=si&session_id=<GUID>
  → …&w=8DF06B0162BC353";src+="&rticks="+1788105746587;

# 3. Trả lời thử thách
GET ov-df.microsoft.com/?session_id=<GUID>&CustomerId=<hằng số>&PageId=si
      &w=<w>&mdt=<epoch ms>&rticks=<rticks>

# 4. Danh sách ngôn ngữ
GET /software-download-connector/api/getskuinformationbyproductedition
      ?profile=606624d44113&ProductEditionId=<mã>&SKU=undefined
      &friendlyFileName=undefined&Locale=en-US&sessionID=<GUID>
  → { "Skus": [ { "Id", "Language", "LocalizedLanguage" } ] }

# 5. Link ký sẵn — bắt buộc kèm header Referer
GET /software-download-connector/api/GetProductDownloadLinksBySku
      ?profile=606624d44113&ProductEditionId=undefined&SKU=<id>
      &friendlyFileName=undefined&Locale=en-US&sessionID=<GUID>
  → { "ProductDownloadOptions": [ { "Uri", "DownloadType": 1 } ] }
```

Năm chi tiết quyết định việc này chạy đúng hay chạy sai một cách âm thầm:

- **Ba bước chống bot, không phải một.** Bản đầu chỉ gọi `vlscppe` rồi đi thẳng tới API. Thiếu
  hai bước `ov-df` thì mọi thứ vẫn *trông như* chạy — bước 4 trả về đủ 38 ngôn ngữ — chỉ bước 5
  bị chặn bằng `ErrorSettings.SentinelReject`. Vì lỗi rơi đúng vào bước cuối nên rất dễ kết luận
  nhầm là Microsoft cấm IP.
- **`DownloadType` là số, không phải chuỗi.** Khai nhầm kiểu thì `serde` vứt cả phản hồi *thành
  công* và ứng dụng báo "dữ liệu không đọc được" cho một lần tải lẽ ra đã xong. Đây là lỗi tự
  che mắt mình: nó chỉ lộ ra sau khi lớp chống bot đã sửa xong, vì trước đó phản hồi luôn là lỗi
  — mà lỗi thì lại đọc được.
- **Bước 5 phải có header `Referer`.** Thiếu là bị từ chối dù phiên đã sạch.
- **Mã product edition đọc từ trang, không ghi cứng.** Mã đổi theo từng bản phát hành, ghi cứng
  thì tới bản sau là hỏng mà không ai biết.
- **Chọn SKU so bằng dấu bằng, không so tiền tố.** Microsoft có cả `English` lẫn
  `English International`, nên so tiền tố sẽ đưa người chọn bản Mỹ sang bản quốc tế *mà vẫn báo
  là đúng*. Cũng vì API gọi bản Mỹ đúng một chữ `English` nên bảng ngôn ngữ trong `languages.rs`
  đổi theo; khâu chọn nhận thêm cả tên ở trường `LocalizedLanguage` để lần đổi tên sau không làm
  hỏng.

Không có mã băm: Microsoft không công bố mã băm ở đâu trong luồng tải của họ, nên
`ResolvedIso.sha256` là `None` với Windows và giao diện chỉ hiện mã băm *tính được* chứ không
nói là "khớp" hay "không khớp". Không có gì để so thì đừng vờ như có.

**Giới hạn thật:** ngay cả khi làm đủ năm bước, Microsoft vẫn từ chối rải rác — chạy năm lần
trên cùng một máy thì vài lần bị chặn, và bản tham chiếu Fido cũng vậy trên chính máy đó. Ứng
dụng vì thế thử lại tối đa ba lần, mỗi lần một phiên mới. Địa chỉ IP của trung tâm dữ liệu bị
siết nặng hơn hẳn sau vài chục lần gọi. Lỗi loại `Type 9` mới là chặn theo IP thật; loại `Type 8`
là phiên chưa qua được lớp chống bot — ứng dụng phân biệt hai thứ đó, vì bảo người dùng đi tắt
VPN trong khi thủ phạm là một bước mình quên làm thì họ sẽ đi sửa mạng nhà mãi mà không xong.

### Mở app là đã có quyền quản trị

Gần như mọi thứ ứng dụng này làm đều đòi quyền quản trị: chia lại phân vùng, ghi thẳng ra
`\\.\PHYSICALDRIVE`, xuất driver khỏi kho của Windows. Mở ở quyền thường thì người dùng đi
được năm bước rồi mới bị chặn.

Nên file `.exe` nhúng manifest khai `requestedExecutionLevel level="requireAdministrator"`:
Windows hiện hộp thoại UAC **trước khi** app khởi động, và lối tắt có khiên nhỏ. Cách còn lại —
tự khởi động lại với quyền cao hơn — phải mở app một lần rồi tắt đi mở lại, vừa chớp nháy vừa
dễ thành vòng lặp nếu người dùng bấm "No".

Khai manifest riêng là **thay thế hẳn** manifest mặc định của `tauri-build`, nên khối
`Microsoft.Windows.Common-Controls` phải chép lại; thiếu nó thì các hộp thoại hệ thống mất kiểu
dáng đời mới. Đánh đổi: tài khoản chuẩn không có mật khẩu quản trị sẽ không mở được app — đúng
đánh đổi, vì không có quyền đó thì cũng không tạo được USB.

### Tải vào thư mục riêng, ghi xong thì dọn

Trước đây bước tải bắt chọn thư mục thủ công rồi để lại một file 3–6 GB nằm đó vĩnh viễn.
Giờ ứng dụng tự tải vào `%LOCALAPPDATA%\GetWinUSB\iso`, và bước ghi có tuỳ chọn **Xoá file
ISO sau khi ghi xong**, mặc định bật.

Hàng rào quan trọng nhất của tính năng này: **chỉ file nằm trong thư mục ứng dụng tự quản
mới xoá được.** File người dùng tự chọn là của họ — xoá nhầm một file ISO 6 GB họ đã tải cả
buổi là thiệt hại không sửa được. `IsoInfo.managed` mang thông tin đó, `download::discard`
từ chối thẳng mọi đường dẫn nằm ngoài, và có test cho cả trường hợp thư mục trùng tiền tố
(`GetWinUSB-cu` không được coi là `GetWinUSB`).

Đánh đổi: đối chiếu từng byte ở bước cuối cần chính file ISO gốc, nên bật xoá tự động
thì mất chức năng đó. Giao diện nói rõ điều này ở cả hai nơi thay vì để người dùng bấm vào
một nút đã biến mất.

### Hai mốc thời gian khác nhau khi tải

`.timeout()` của reqwest là hạn chót cho **toàn bộ** request, tính cả thời gian tải hết
body — không phải timeout kết nối. Dùng chung một client 60 giây cho cả request nhỏ lẫn
việc tải ISO nghĩa là mọi file đều bị huỷ ở giây thứ 60, vì không file 3–6 GB nào tải xong
trong ngần ấy thời gian. Đây là lý do tính năng tải tự động chưa bao giờ chạy được, với
cả Windows lẫn Linux.

Nên có hai client tách bạch: `client()` cho các request nhỏ (trang HTML, file mã băm) giữ
hạn chót 60 giây, còn `download_client()` bỏ hẳn hạn chót tổng và thay bằng
`connect_timeout` cho lúc bắt tay và `read_timeout` cho khoảng lặng giữa hai khối dữ liệu.
Mạng chậm vẫn tải được; kết nối chết thật vẫn bị cắt sau một phút không nhận được gì.

Ba test khoá lại đúng sự khác biệt này, dùng một máy chủ tí hon nhỏ giọt dữ liệu ngay
trong test — không gọi mạng, chạy hết 3 giây.

### Link tải Linux tra qua file mã băm

Ứng dụng **không** ghi cứng link ISO của distro. Tên file đổi theo từng bản vá nhỏ —
`ubuntu-24.04.2` thành `24.04.3` là link cũ chết ngay, mà chết im lặng: người dùng chỉ
thấy "tải thất bại" chứ không biết vì sao.

Thay vào đó danh mục lưu địa chỉ file `SHA256SUMS`, vốn nằm ở một thư mục cố định. Đọc
file đó một lần được cả hai thứ: tên file ISO hiện hành, và mã băm chính thức để đối
chiếu ngay sau khi tải xong. Bản vá nhỏ mới ra thì tự tải đúng file mới, không phải sửa
mã. Sáu trong chín bản dùng được đường này; ba bản còn lại (Fedora, Pop!_OS, Zorin) phát
link qua trang trung gian nên chỉ mở trang chính thức.

Đối chiếu mã băm chạy tự động sau khi tải, không đợi người dùng bấm: file ISO tải dở sẽ
ghi ra một chiếc USB không boot được, và lúc đó rất khó đoán nguyên nhân nằm ở đâu.

### Vì sao ISO Linux phải ghi nguyên khối

ISO của các distro là *hybrid ISO*: bảng phân vùng và mã khởi động nằm ngay trong chính
file ảnh đĩa. Chép từng file ra một phân vùng FAT32 như cách làm với bộ cài Windows sẽ
hỏng, vì bootloader (isolinux/GRUB) trông chờ đúng bố cục ISO9660 và đúng nhãn volume mà
nó được dựng cùng — máy sẽ báo không tìm thấy thiết bị khởi động, hoặc dừng giữa chừng ở
initramfs.

Vì ghi từ byte 0 nên thao tác này xoá luôn bảng phân vùng cũ. Không cần và cũng không
được format trước, nên **luồng Linux không chạy bước chia phân vùng nào cả** — chỉ một thao
tác ghi, ba chặng, và ô xác nhận xoá dữ liệu nằm ngay tại đó.

Trên Windows, ghi thẳng ra `\\.\PHYSICALDRIVE<n>` cần ba việc đúng thứ tự: `Clear-Disk`
để hệ điều hành nhả khoá volume, đưa ổ về offline để Windows không gắn phân vùng mới vào
giữa chừng, rồi ghi theo bội số sector (khối cuối được đệm 0 cho tròn).

### Không có Windows tiếng Việt

Microsoft chưa từng phát hành ISO Windows tiếng Việt. Trang tải chính thức không có mục
nào cho tiếng Việt, và trước đây ứng dụng lại ghi cứng `"Vietnamese"` khi hỏi link tải và
ghi cứng `"Tiếng Việt (vi-vn)"` ở phần gợi ý — tức là hướng người dùng đi chọn một thứ
không tồn tại.

Từ đó ra hai khái niệm phải tách bạch, và gộp chúng lại chính là gốc của lỗi:

| | Bị giới hạn bởi gì | Tiếng Việt dùng được không |
|---|---|---|
| Ngôn ngữ hiển thị (`UILanguage`) | Những gì nằm trong file ISO | Không |
| Định dạng vùng, bàn phím (`SystemLocale`, `UserLocale`, `InputLocale`) | Không giới hạn | **Có** |

Nên bước Phiên bản có bộ chọn ngôn ngữ bộ cài — 38 ngôn ngữ Microsoft thật sự phát hành,
không có tiếng Việt, kèm giải thích ngay tại chỗ. Còn bước Ghi bộ cài thì ngôn ngữ hiển
thị **khoá theo ISO đã chọn** (không sửa được, vì sửa cũng vô nghĩa), và định dạng vùng
vẫn chọn được Việt Nam. Đây đúng là thứ người dùng Việt Nam cần: Windows tiếng Anh nhưng
ngày tháng, tiền tệ và bàn phím theo Việt Nam.

Bảng ngôn ngữ nằm ở `languages.rs` và được đẩy sang giao diện qua một lệnh Tauri, thay vì
giao diện giữ một bản sao thứ hai rồi lệch dần khỏi bản backend dùng để khớp SKU.

### Bỏ qua màn hình cài đặt ban đầu

Windows Setup tự tìm file `autounattend.xml` ở thư mục gốc của thiết bị rời. Ứng dụng sinh
file này theo cấu hình bạn chọn: ngôn ngữ, múi giờ, tên máy, tài khoản cục bộ, và tuỳ chọn
bỏ qua kiểm tra TPM. Kết quả là toàn bộ chuỗi màn hình hỏi đáp sau khi cài (vùng, bàn phím,
mạng, giấy phép, tài khoản Microsoft, quyền riêng tư) được trả lời sẵn.

File này chủ ý **không** chứa `DiskConfiguration`. Thêm vào thì Setup sẽ tự xoá và chia lại
ổ cứng đích mà không hỏi lại lần nào — quá nguy hiểm cho một công cụ mà người dùng có thể
cắm nhầm máy. Có hẳn một test khoá điều này lại để lần mở rộng sau không vô tình thêm vào.

### Kèm driver để cài xong là có Wi-Fi

Đây là vòng luẩn quẩn quen thuộc: cài lại Windows xong thì máy mất Wi-Fi, mà muốn tải driver
Wi-Fi thì lại cần Wi-Fi. Chỉ phá được bằng cách đưa driver lên USB từ trước.

**Cách đưa driver vào.** Windows Setup tự tìm thư mục tên `$WinPEDriver$` ở gốc ổ đĩa rời và
cài mọi driver trong đó vào ảnh Windows đang cài. Không phải sửa bộ cài, và quan trọng hơn:
không phải biết ổ USB sẽ mang chữ cái nào lúc WinPE chạy. Đường còn lại — khai `DriverPaths`
trong `autounattend.xml` — bắt buộc ghi một đường dẫn tuyệt đối kiểu `E:\Drivers`, mà chữ cái
đó thì không ai đoán trước được, nên ứng dụng không dùng.

**Driver lấy từ đâu.** Hai nguồn, cùng một cỗ máy phân tích phía sau:

- **Xuất từ chính máy đang chạy** (`Export-WindowsDriver -Online`, cần quyền quản trị). Đây là
  nguồn đáng tin nhất khi người dùng cài lại chính chiếc máy của mình: không phải đoán model,
  không phụ thuộc trang tải nào còn sống. Cũng khớp đúng giả định mà cả engine gợi ý đang dựa
  vào — máy đang dùng chính là máy sắp cài.
- **Một thư mục driver có sẵn** người dùng tự tải về và giải nén. Ứng dụng tự tìm mọi `.inf`
  bên trong, kể cả nằm sâu nhiều tầng.

**Lọc theo nhóm, và vì sao phải lọc.** Đánh đổi của `$WinPEDriver$` là Setup cài *tất cả*
driver trong thư mục, bất kể máy có thiết bị đó hay không. Nên bộ lọc ở đây không phải để tiết
kiệm dung lượng mà để giảm rủi ro — một driver điều khiển ổ đĩa sai có thể làm máy không khởi
động được. Ba mức: chỉ mạng và ổ đĩa; khuyến nghị (thêm chipset, USB, bàn phím, chuột, âm
thanh); tất cả. Card đồ hoạ cố ý nằm ngoài mức khuyến nghị: đó là nhóm hay gây lỗi nhất khi bị
nhồi sẵn, mà thiếu nó thì Windows vẫn chạy bằng driver cơ bản rồi tự cập nhật sau.

**Đối chiếu với phần cứng thật.** Phần đáng giá nhất của bước này không phải việc chép file mà
là câu trả lời "chiếc USB này có driver cho card Wi-Fi của tôi hay không". Ứng dụng bóc mã phần
cứng (`PCI\VEN_8086&DEV_51F0&SUBSYS_00748086`) từ các section models trong file `.inf`, đọc mã
phần cứng thật của từng card mạng và ổ đĩa qua `Get-PnpDeviceProperty`, rồi so hai bên. So xuôi
một chiều — mã thiết bị bắt đầu bằng mã trong INF — nên bắt được trường hợp Windows báo thêm
đuôi `&REV_01` mà INF không ghi, nhưng **không** nhận nhầm một INF khai `SUBSYS` của hãng khác:
Windows sẽ không cài nó, nên ứng dụng cũng không được nói là đã có.

Vài chi tiết nhỏ nhưng cần thiết, mỗi cái đều có test khoá lại:

- Đơn vị chép là **cả thư mục** chứa `.inf`, không phải từng file: thiếu `.sys` hay `.cat` nằm
  cạnh là Setup từ chối cài vì chữ ký không khớp.
- INF có thể là UTF-16LE, UTF-8, hay một bảng mã 8-bit đời cũ. Đoán sai bảng mã thì không đọc
  ra nổi dòng `Class=` và gói driver đúng bị loại vì tưởng là không đọc được.
- Nhiều INF chỉ ghi `ClassGuid` mà không ghi `Class`, nên có bảng tra GUID sang tên nhóm.
- Thư mục chỉ chứa `.exe`/`.msi` được **đếm riêng và nói ra**. Đây là hiểu lầm phổ biến nhất:
  tải "driver Wi-Fi" từ trang hãng và nhận về một file cài đặt — thứ Setup không nhồi vào ảnh
  cài được. Im lặng bỏ qua thì người dùng tưởng đã xong.
- Tên thư mục trùng nhau được đánh số, và ký tự lạ bị thay bằng `_` trước khi chép sang FAT32.

### File trên ISO là chỉ-đọc, và điều đó làm hỏng lần ghi thứ hai

Mọi file trên một ổ ISO đã gắn đều mang thuộc tính ReadOnly — hệ thống file ISO9660/UDF vốn chỉ
đọc. Thao tác chép của Windows (`CopyFile`, và `File::Copy` của .NET nằm trên nó) chép luôn
thuộc tính đó sang đích. Nên sau một lần ghi, mọi file trên USB đều là chỉ-đọc.

Lần ghi thứ hai lên cùng chiếc USB mà không format lại sẽ ghi đè lên chính những file đó, và
`CopyFile` trả về `ERROR_ACCESS_DENIED`. .NET diễn giải thành *"Access to the path '...' is
denied"*, và vì `__chunk_data` (file mới có trong ISO Windows 11 dựng từ UUP, nằm ngay gốc ISO)
được liệt kê đầu tiên, lỗi rơi đúng vào file đầu tiên — thanh tiến trình chưa kịp nhích một
lần nào.

Bản sửa gỡ thuộc tính của file đích trước khi ghi, và trả file vừa chép về `Normal` để lần sau
không vướng lại. Cùng một lỗi đó có ở ba chỗ khác nên sửa luôn: các mảnh `install*.swm` của lần
trước (DISM từ chối ghi đè), `autounattend.xml`, và các gói driver chép vào `$WinPEDriver$`.

Còn một giới hạn chưa xử lý: `needs_split` chỉ xét `install.wim`/`install.esd`. Nếu một file
*khác* trên ISO vượt 4 GB thì bước chép sẽ chết giữa chừng trên FAT32 thay vì báo trước — chưa
gặp ISO nào như vậy, nhưng đây là chỗ nên chặn sớm.

### Tốc độ ghi đo bằng cửa sổ trượt

Ghi ra USB không chảy đều: chép mấy file nhỏ thì xong tức thì, tới lúc ổ đẩy bộ đệm ra bộ nhớ
flash thì đứng im vài giây. Lấy hiệu hai mẫu liền nhau sẽ cho ra một con số nhảy loạn giữa 0 và
vài trăm MB/s — vô dụng với người đang ngồi nhìn màn hình, và tệ hơn là hiện "0 B/s" đúng lúc
ổ đang bận nhất khiến người dùng tưởng máy treo.

Nên tốc độ tính trên **cửa sổ trượt ba giây**: tổng số byte tăng thêm trong ba giây gần nhất
chia cho đúng khoảng thời gian đó. Một nhịp khựng không làm con số rơi về 0, còn chậm hẳn thì
vẫn thấy ngay khi cửa sổ trượt qua đoạn nhanh. Dưới 400 ms thì không báo gì cả: khoảng thời
gian quá ngắn để chia, sai số đồng hồ lấn át phép đo, và một con số bịa ra từ vài chục mili
giây đầu còn tệ hơn là chưa có số.

Đo ở cả ba chỗ thật sự đếm được byte: chép bộ cài Windows, tách `install.wim`, và ghi nguyên
khối cho Linux — cộng thêm phần đọc lại từng byte ở bước cuối. Hai chặng chia phân vùng và
phần đối chiếu theo *số file* thì không có byte nào để đo, và `speed_bps == 0` chính là dấu hiệu
để giao diện ẩn phần tốc độ đi thay vì hiện một con số vô nghĩa.

Chi tiết đáng nói về cách nối vào mã cũ: hàm ghi báo tiến trình qua một closure `emit` dùng
chung cho hơn ba mươi chỗ gọi, mà chỉ ba trong số đó đếm được byte. Thêm một tham số tốc độ vào
closure nghĩa là sửa cả ba mươi chỗ để truyền một giá trị rỗng. Thay vào đó có `rate::Slot` —
một ô nhớ dùng-một-lần: chặng nào đo được thì đặt số liệu vào ngay trước khi gọi `emit`, và
`emit` lấy ra bằng `take()` nên ô nhớ trở lại rỗng, số liệu không bao giờ dính sang lần báo sau.
Có test khoá đúng tính chất đó.

Thanh tiến trình cũng chuyển lên ngay dưới tiêu đề bước, và trang tự cuộn tới nó khi bắt đầu
ghi. Trước đây nó nằm cuối trang: bấm nút xong màn hình không có gì thay đổi, người dùng phải
tự cuộn xuống mới biết là đang chạy.

### Script PowerShell là chuỗi ký tự cho tới lúc chạy

Toàn bộ thao tác với đĩa đi qua PowerShell, và với Rust thì mỗi script chỉ là một chuỗi ký
tự — biên dịch sạch, test xanh, mà vẫn có thể sai cú pháp. Một lỗi thật đã lọt qua đúng cách
đó:

```powershell
elseif (Test-Path (Join-Path $root 'efi\boot\bootia32.efi') -and -not (Test-Path ...))
```

PowerShell đọc `-and` là **tham số** của `Test-Path` chứ không phải toán tử logic, nên cả lệnh
chết với *"A parameter cannot be found that matches parameter name 'and'"*. Nhánh `elseif` đó
chạy với mọi ISO không phải ARM64 — tức là mọi ISO Windows thông thường — nên bước đọc nội
dung ISO chưa bao giờ chạy được, và không có gì trong CI phát hiện ra.

Bản sửa là tách mỗi lệnh ra biến riêng. Nhưng phần đáng kể hơn là cái chặn nó quay lại: một
test đọc thẳng **mã nguồn**, bóc mọi chuỗi thô `r#"…"#` trông như PowerShell trong mọi module,
rồi soát từng cái. Cách nhận biết là đếm độ sâu ngoặc: gặp `-and` mà cùng độ sâu với một tên
lệnh nghĩa là toán tử đang nằm trong danh sách tham số của lệnh đó. Viết đúng thì lệnh phải
có ngoặc riêng, tức sâu hơn một bậc. Đọc mã nguồn thay vì liệt kê tên từng hằng số để script
mới thêm vào bất kỳ module nào cũng tự được soát.

Hai chỗ khác trong cùng bước format cũng được làm chặt lại, đều thuộc loại "chưa từng chạy nên
chưa từng lộ":

- `Clear-Disk` từ chối ổ đang ở trạng thái RAW. Đó không phải lỗi cần dừng — đích đến của bước
  đó chính là RAW. Và `Initialize-Disk` chỉ chạy được với ổ RAW, ổ đã có kiểu phân vùng thì
  phải đổi bằng `Set-Disk`.
- Đối tượng `New-Partition` trả về là ảnh chụp *trước* lúc Windows gán ký tự ổ đĩa, nên
  `$part.DriveLetter` thường rỗng. Phải hỏi lại hệ thống mới có ký tự thật; không thì mọi bước
  sau ghi vào một đường dẫn `":\"`.

### Khung giao diện co theo cửa sổ

Ba thay đổi nhỏ giải quyết phần lớn chuyện bố cục ở các cỡ cửa sổ khác nhau:

- **Cột nội dung có trần rộng 1280px và luôn nằm giữa.** Không có trần thì trên màn 2K mọi
  thứ bị kéo dài hết bề ngang: dòng chữ dài quá tầm mắt, lưới thẻ tự tách thành sáu bảy cột
  mỏng dính, và nút bấm bị đẩy ra tận mép màn hình. Trần này cố tình là một con số cố định
  chứ không phải `clamp(…, vw, …)` — bất kỳ vế nào theo `vw` cũng làm cột hẹp hơn chỗ đang
  có ở các cửa sổ cỡ vừa, tức là tự tạo ra đúng thứ khoảng trống nó sinh ra để tránh.
- **Thanh "Quay lại / Tiếp tục" dính đáy vùng nội dung.** Trước đây hai nút nằm cuối vùng
  cuộn, nên ở bước dài phải cuộn tới đáy mới thấy. Thanh này dùng lại đúng lớp `.shell` nên
  nút của nó thẳng hàng với hai mép nội dung phía trên, không phải hai mép màn hình.
- **Nội dung ngắn thì căn giữa theo chiều dọc** (`align-content: safe center`). Từ khoá
  `safe` là phần thiết yếu: thiếu nó thì nội dung dài hơn khung sẽ bị cắt mất phần đầu và
  không cuộn ngược lên được.

Kèm theo là ba mốc co giãn: dưới 980px cột bước dựng đứng đổi thành dải ngang cuộn được ở
trên; dưới 720px chiều cao thì cắt bớt khoảng đệm; từ 1900px trở lên nới cỡ chữ và khoảng
đệm để cột nội dung 1280px không bị hụt giữa màn hình rộng. Cửa sổ tối thiểu hạ xuống
840×600 để những mốc này thật sự với tới được.

---

## Cấu trúc

```
src-tauri/src/
  ps.rs         Cầu nối PowerShell (EncodedCommand, đọc stdout theo dòng, kiểm tra quyền admin)
  rate.rs       Đo tốc độ ghi bằng cửa sổ trượt và thời gian còn lại
  usb.rs        Nhận diện ổ USB + vòng theo dõi cắm/rút
  hardware.rs   Quét CPU/RAM/TPM/Secure Boot/firmware
  cpu.rs        Suy luận CPU có nằm trong danh sách hỗ trợ Windows 11 không
  catalog.rs    Danh mục các bản Windows + tính vòng đời theo ngày hiện tại
  languages.rs  Ngôn ngữ bộ cài Microsoft phát hành, và locale chỉ dùng cho vùng
  distro.rs     Danh mục hệ điều hành mã nguồn mở + engine chấm điểm theo RAM
  catalog_sync.rs  Đọc bảng vòng đời từ trang release-health của Microsoft
  checks.rs     Đối chiếu 13 thành phần phần cứng với yêu cầu Windows 11
  recommend.rs  Engine chấm điểm và xếp hạng
  download.rs   Giải link ISO Windows/Linux + tải có tiến trình, có resume, tính SHA-256
  drivers.rs    Đọc file INF, lọc theo nhóm, đối chiếu mã phần cứng, chép vào USB
  writer.rs     Format ổ, chép file, tách install.wim, ghi bootsect, ghi nguyên khối
  verify.rs     Kiểm tra USB sau khi ghi: cấu trúc khởi động + đọc lại đối chiếu
  unattend.rs   Sinh autounattend.xml để bỏ qua màn hình cài đặt ban đầu
  lib.rs        Các lệnh Tauri và vòng theo dõi nền

src/
  App.tsx           Máy trạng thái theo tên bước; sáu bước, chung cho cả hai họ
  types.ts          Khớp 1-1 với struct Rust
  lib/api.ts        Bọc invoke + listen
  components/       Titlebar, các màn hình bước, các mảnh dùng chung (`Fold`, `Why`…)
  styles.css        Hệ thống thiết kế (biến CSS, sáng/tối, khung co theo cửa sổ)
```

---

## Bốn quyết định kỹ thuật đáng chú ý

### Danh mục phiên bản tự cập nhật

Ghi cứng danh sách phiên bản Windows vào mã nguồn có hai kiểu hỏng khác nhau, và kiểu thứ
hai nguy hiểm hơn nhiều:

1. Bản mới ra thì app không biết — người dùng thấy thiếu, dễ nhận ra.
2. Bản cũ hết hỗ trợ mà app vẫn nói còn hạn — **app nói dối mà không ai biết**.

Kiểu thứ hai xử lý bằng cách tính từ đồng hồ hệ thống: ngày lưu dạng ISO, mọi kết luận về
vòng đời đều so với hôm nay. Kiểu thứ nhất xử lý bằng `catalog_sync.rs` — đọc bảng vòng đời
trên trang release-health của Microsoft lúc khởi động.

Microsoft không có API JSON ổn định cho dữ liệu này nên phải bóc tách HTML, và HTML thì đổi
lúc nào không báo. Ba nguyên tắc chống vỡ:

- **Ánh xạ cột theo tiêu đề, không theo vị trí.** Bảng có cả cột "Latest revision date" chen
  giữa và hai cột "End of servicing" (Home/Pro và Enterprise, lệch nhau một năm). Đọc theo
  thứ tự cột thì sẽ lấy nhầm ngày mà không hề báo lỗi.
- **Hợp nhất chứ không thay thế.** Trang của Microsoft chỉ có build và ngày tháng; yêu cầu
  TPM hay cách lấy ISO vẫn lấy từ bảng nhúng. Đọc hụt một dòng thì mất một bản cập nhật,
  không mất cả danh mục.
- **Nghi ngờ thì bỏ qua.** Dòng thiếu ngày, bản chỉ dành cho máy mới xuất xưởng (26H1), hay
  bản đã quá hạn mà app chưa từng biết — đều bị loại thay vì đoán bừa.

Nhờ vậy một bản chưa tồn tại lúc viết mã — 26H2, hay bất cứ mã nào sau đó — vẫn tự vào danh
sách chọn, mang nhãn "Mới phát hiện" và yêu cầu phần cứng suy theo bản Windows 11 gần nhất
đã biết. Bước tải cũng không cần sửa gì: trang tải và mã phiên bản đều suy từ mã bản
(`win11-26h2` → trang Windows 11, mã `26H2`) chứ không ghi cứng ở đâu cả.

**Trang tải chỉ phục vụ đúng một bản, và app phải nói ra bản nào.** Mục "multi-edition ISO"
trên trang của Microsoft luôn trỏ tới bản hiện hành — không có tham số nào để đòi bản cũ hơn
hay mới hơn. Trước đây app bỏ qua điều đó: chọn 24H2 thì vẫn nhận về ISO 25H2, tên file nói
một đằng nhãn trong app nói một nẻo. Nay `ProductDisplayName` trong phản hồi của Microsoft
("Windows 11 25H2\_\_V2") được đối chiếu với bản đã chọn, và hai chiều lệch được xử lý khác
nhau:

| Tình huống | App làm gì |
|---|---|
| Chọn bản cũ hơn bản Microsoft đang phát | Từ chối, nói rõ đang có bản nào và mời chọn lại |
| Chọn đúng bản mới nhất app biết, Microsoft đã ra bản mới hơn | Nhận link, và hiện một ghi chú rằng file tải về là bản mới hơn — danh mục trong máy cũ, không phải người dùng chọn nhầm |

Ranh giới giữa hai dòng đó là chỗ dễ làm sai nhất: chặn cả hai thì tới lúc 26H2 lên trang mà
máy chưa đồng bộ được danh mục, người dùng mất luôn đường lấy bản mới nhất; nhận cả hai thì
app lặng lẽ đưa ra một file khác thứ người ta chọn.

Phiên bản mới phát hiện được đánh dấu `discovered` và hiện huy hiệu riêng, vì yêu cầu phần
cứng của nó là suy theo bản liền trước chứ chưa được xác nhận.

Kết quả lưu đệm vào `%LOCALAPPDATA%\GetWinUSB\catalog.json`, nên lần mở sau không có mạng
vẫn dùng được dữ liệu mới nhất từng đọc. Màn hình gợi ý luôn ghi rõ dữ liệu đến từ đâu và
cập nhật ngày nào.

## Ba quyết định kỹ thuật khác

### Vì sao gọi PowerShell thay vì bind WMI trực tiếp

Crate `wmi` nhanh hơn nhưng chỉ biên dịch được trên Windows, và mỗi truy vấn mới lại cần
định nghĩa struct COM. Đi qua PowerShell thì toàn bộ mã nguồn biên dịch được trên mọi nền
tảng — mã hoá lệnh bằng `-EncodedCommand` (base64 của UTF-16LE) nên không có chuyện lỗi
escape dấu nháy. Chi phí duy nhất là ~300ms khởi động tiến trình, và vòng theo dõi USB đã
tránh được bằng cách giữ đúng một tiến trình chạy suốt.

### Giới hạn FAT32 và cách xử lý

FAT32 không chứa nổi file quá 4 GB, mà `install.wim` của Windows 11 thường nặng 5–6 GB.
Ứng dụng dùng `DISM /Split-Image` cắt thành các mảnh `install.swm` 3800 MB — Windows Setup
nhận diện tự động, người dùng không phải làm gì thêm. Đây là đường đi được Microsoft hỗ trợ
chính thức, không phải mẹo vặt.

DISM có hỏng, và bước này phải nói được **vì sao** hỏng. Bản đầu nuốt mất cả ba thứ cần
thiết:

- Output của DISM chảy thẳng vào stdout chung rồi bị bỏ qua vì không mang tiền tố `GWU:`.
  Nay nó được hứng ra file riêng, và khi hỏng thì mấy dòng có nội dung cuối cùng — thường là
  `Error: 0x…` kèm một câu tiếng Anh — được ghép vào lời báo lỗi.
- `Start-Process -PassThru` trên PowerShell 5.1 hay trả về `ExitCode` rỗng, nên câu báo lỗi
  từng kết thúc bằng "mã lỗi " rồi bỏ lửng. Chạm vào `$p.Handle` ngay sau khi khởi chạy thì
  .NET giữ lại thông tin tiến trình; ngoài ra còn xét cả kết quả thật trên ổ, vì không có
  mảnh `.swm` nào thì chắc chắn là hỏng bất kể mã lỗi nói gì.
- Khởi chạy `dism.exe` thất bại thì `$p` là `$null`, mà `-not $null.HasExited` cho ra `TRUE`
  — vòng lặp theo dõi tiến trình quay vô tận và cả bước ghi treo cứng, không báo gì. Nay có
  một dòng chặn ngay tại đó.

Lời báo lỗi cũng chỉ luôn đường vòng dùng được ngay: đổi sang **MBR + NTFS** thì không có
giới hạn 4 GB nên không phải tách file. Kiểu phân vùng nằm trong khối gập ngay tại bước Ghi.

Ngoài ra `Format-Volume` của Windows từ chối tạo FAT32 lớn hơn 32 GB, nên với USB dung lượng
lớn, phân vùng boot bị cắt ở đúng 32 GB và phần dư được tạo thành một phân vùng NTFS tên
`DATA` — thay vì bỏ phí vài chục GB.

### Ba lớp bảo vệ chống xoá nhầm ổ

Đây là phần duy nhất có thể làm mất dữ liệu, nên có ba hàng rào độc lập:

1. **Lọc lúc liệt kê** — chỉ ổ có `BusType = USB` mới hiện ra; ổ hệ thống, ổ boot và ổ
   chỉ đọc bị loại ngay trên giao diện.
2. **Vân tay ổ đĩa** — ngay trước khi ghi, ứng dụng đọc lại `model|serial|dung lượng` và so
   với giá trị ghi nhận lúc người dùng bấm chọn. Rút ổ ra cắm ổ khác vào cùng vị trí thì
   số hiệu ổ có thể trùng, nhưng vân tay thì không — và thao tác bị từ chối.
3. **Kiểm tra lại trong PowerShell** — ngay trước lệnh `Clear-Disk`, script kiểm tra
   `IsSystem`, `IsBoot`, `BusType` một lần nữa.

---

## Build

Cần Windows 10/11, [Rust](https://rustup.rs), Node 18+, và
[Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
kèm WebView2 (Windows 11 có sẵn).

```bash
npm install
npm run tauri dev      # chạy thử, hot reload
npm run tauri build    # xuất bộ cài .exe vào src-tauri/target/release/bundle/nsis/
```

### Dựng bản phát hành

Đường chính thống là workflow `.github/workflows/release.yml`: nó chạy trên runner
Windows thật, chạy test rồi mới đóng gói, và ghi kèm mã băm SHA-256 của bộ cài. Đẩy một
tag `v*` là nó tự chạy, hoặc bấm tay qua "Run workflow".

Có thể cross-compile từ Linux khi cần một bản thử nhanh:

```bash
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin
sudo apt-get install nsis clang lld     # makensis + trình liên kết
npm run tauri build -- --runner cargo-xwin --target x86_64-pc-windows-msvc
```

Đường này Tauri ghi rõ là **thử nghiệm** và có hai giới hạn thật:

- **Không ký số được.** Trình đóng gói chỉ ký trên máy Windows, nên bộ cài ra từ Linux
  luôn chưa ký — Windows SmartScreen sẽ chặn ở lần chạy đầu.
- **Không chạy thử được.** Máy dựng không phải Windows nên không có cách nào biết bộ cài
  có chạy đúng hay không cho tới khi mang sang máy thật.

Dùng nó để kiểm tra "có biên dịch được không", còn bản giao cho người dùng thì lấy từ
workflow.

Chạy test cho phần logic (bộ nhận diện CPU và engine gợi ý):

```bash
cd src-tauri && cargo test
```

---

## Trạng thái hiện tại

Đã kiểm chứng:

- 160 test đơn vị cho `cpu.rs`, `catalog.rs`, `catalog_sync.rs`, `checks.rs`, `recommend.rs`,
  `distro.rs`, `download.rs`, `drivers.rs`, `languages.rs`, `unattend.rs`, `verify.rs`,
  `writer.rs` — chạy xanh
- Luồng tải ISO Windows chạy thật với Microsoft, lấy về link ký sẵn (`cargo test live_probe --
  --ignored`). Đây là thứ duy nhất bắt được lỗi kiểu "thiếu một bước chống bot"; test đọc phản
  hồi cắt sẵn vẫn xanh trong khi tính năng hỏng hoàn toàn ngoài đời
- Sáu địa chỉ `SHA256SUMS` trong danh mục distro đã kiểm chứng giải ra đúng file ISO
  hiện hành
- Toàn bộ file Rust qua được kiểm tra cú pháp
- Frontend TypeScript qua `tsc` ở chế độ `strict` không lỗi
- Cả sáu bước của cả hai luồng chạy qua trong trình duyệt với backend giả, dữ liệu do chính
  engine Rust sinh ra — không có lỗi runtime nào

Chưa kiểm chứng được (môi trường dựng ứng dụng không có Windows và không tải được thư viện):

- Chạy thật trên Windows. Phần Rust đã `cargo check` sạch cho `x86_64-pc-windows-msvc`
  (không còn cảnh báo nào), nhưng biên dịch được không đồng nghĩa với chạy đúng
- Các đoạn PowerShell chạy trên máy thật, **kể cả đoạn ghi nguyên khối và đoạn đọc lại
  `\\.\PHYSICALDRIVE<n>`** — hãy thử trên một ổ USB không chứa dữ liệu quan trọng
- Việc xuất driver (`Export-WindowsDriver`) và đọc mã phần cứng (`Get-PnpDeviceProperty`).
  Phần phân tích INF và đối chiếu mã thì chạy được ở mọi nơi và có test; phần gọi Windows thì
  chưa
- Đường tải tự động của các bản Linux trên máy người dùng thật. Máy dựng bị `releases.ubuntu.com`
  trả 403 (Canonical chặn dải IP trung tâm dữ liệu), nên phần này chỉ kiểm chứng được tới mức
  giải link và mã băm; giao diện luôn có sẵn hai đường lui: chọn file ISO có sẵn, và mở trang
  tải chính thức trong trình duyệt.

**Trước khi thử tính năng ghi USB, hãy dùng một ổ USB không chứa dữ liệu quan trọng.**

---

## Dữ liệu phiên bản Windows

Cập nhật tới tháng 8/2026. Muốn thêm bản mới chỉ cần sửa mảng `CATALOG` trong
`src-tauri/src/catalog.rs`.

| Phiên bản | Build | Hết hỗ trợ |
|---|---|---|
| Windows 11 25H2 | 26200 | 12/10/2027 |
| Windows 11 24H2 | 26100 | 13/10/2026 |
| Windows 11 IoT Enterprise LTSC 2024 | 26100 | 10/10/2034 |
| Windows 10 IoT Enterprise LTSC 2021 | 19044 | 13/01/2032 |
| Windows 10 22H2 | 19045 | đã hết 14/10/2025 |

Windows 11 26H2 dự kiến phát hành tháng 10/2026 — khi có build chính thức thì thêm vào đầu
danh mục và cho `win11-25h2` lùi một bậc ưu tiên trong `recommend.rs`.

---

## Dữ liệu bản Linux

Bảng nhúng chốt ngày 29/08/2026, khai báo trong `CATALOG` của `src-tauri/src/distro.rs`.
Khác với danh mục Windows, bảng này **không tự đồng bộ** — các dự án Linux ra bản mới theo
nhịp riêng, không có một trang vòng đời chung nào để đọc. Số hiệu phiên bản và ngày hết hỗ
trợ vì thế cần rà lại theo định kỳ; link tải thì luôn đúng nhờ cách tra qua file mã băm.

| Bản | Desktop | RAM khuyến nghị | Hết hỗ trợ |
|---|---|---|---|
| Ubuntu 24.04 LTS | GNOME | 8 GB | 31/05/2029 |
| Linux Mint 22 Cinnamon | Cinnamon | 4 GB | 30/04/2029 |
| Linux Mint 22 XFCE | XFCE | 3 GB | 30/04/2029 |
| Debian 13 "Trixie" | GNOME | 4 GB | 30/06/2030 |
| Fedora Workstation 43 | GNOME | 8 GB | 01/12/2026 |
| Pop!_OS 22.04 LTS | COSMIC | 8 GB | 30/04/2027 |
| Zorin OS 17 Core | GNOME tuỳ biến | 4 GB | 30/04/2027 |
| Lubuntu 24.04 LTS | LXQt | 2 GB | 30/04/2027 |
| Arch Linux | không có sẵn | 2 GB | rolling |
