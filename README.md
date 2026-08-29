# Get WinUSB

Ứng dụng Windows tự nhận diện ổ USB đang cắm, quét cấu hình máy, gợi ý hệ điều hành
phù hợp nhất — Windows hoặc một trong chín bản Linux — rồi tạo luôn USB cài đặt.

Tauri 2 + React 18 + TypeScript. Backend Rust, giao diện kính xếp lớp có chế độ sáng/tối.

---

## Ứng dụng làm gì

**0. Chọn hệ điều hành** — bước đầu tiên, và nó quyết định hình dạng của mọi bước sau.
Hai họ hệ điều hành được tạo USB theo hai cách khác hẳn nhau, không phải một cách có
tham số. Xem "Vì sao ISO Linux phải ghi nguyên khối" bên dưới.

| | Windows | Linux |
|---|---|---|
| Cách ghi | Chép file lên phân vùng đã format | Ghi nguyên khối, tương đương `dd` |
| Bước Format | Có, tách riêng, có xác nhận | Không — thao tác ghi đã bao gồm |
| Cài đặt tự động | `autounattend.xml` | Không có |
| Ổ USB tối thiểu | 8 GB | Vừa đúng cỡ file ISO |
| Kiểm tra sau khi ghi | Đối chiếu từng file với ISO | Băm lại từng byte đã ghi |

**1. Nhận diện USB** — một tiến trình PowerShell chạy nền suốt vòng đời ứng dụng, cứ 3 giây
in danh sách ổ USB dưới dạng JSON. Rust đọc từng dòng và chỉ đẩy sự kiện lên giao diện khi
danh sách thực sự đổi. Cắm hay rút ổ thì danh sách tự cập nhật, không cần bấm gì.

**2. Quét phần cứng** — 13 mục đối chiếu với yêu cầu chính thức của Windows 11, mỗi mục một
dấu trạng thái: bộ xử lý (kiểu, số nhân, xung nhịp, kiến trúc), RAM, dung lượng ổ, chế độ
khởi động, Secure Boot, TPM, card đồ hoạ, và ba mục màn hình (độ phân giải, kích thước,
độ sâu màu).

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

**3. Gợi ý phiên bản** — đối chiếu cấu hình với yêu cầu của từng bản Windows rồi chấm điểm
0–100. Mỗi bản được gắn một trong bốn kết luận:

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

**4. Nguồn bộ cài** — chọn file ISO có sẵn, tải tự động từ Microsoft (có nối tiếp khi đứt
mạng), hoặc mở trang tải chính thức. Đọc luôn nội dung ISO: có những bản Windows nào,
kiến trúc gì, install.wim nặng bao nhiêu. Có nút tính SHA-256 để đối chiếu.

**5. Format USB** — xoá và chia lại phân vùng, tách riêng thành một bước có xác nhận của
chính nó. Đây là thao tác duy nhất trong ứng dụng làm mất dữ liệu, nên nó không được nấp
bên trong một nút "tạo USB" chung.

**6. Ghi bộ cài** — chép file, tách install.wim nếu cần, ghi mã khởi động, và ghi
`autounattend.xml`. Tiến trình hiện theo byte thực tế ở cả sáu chặng, kể cả khi đang chép
một file 5 GB đơn lẻ.

**7. Kiểm tra khởi động** — đọc lại chính chiếc USB vừa ghi. Xem "Vì sao cần bước kiểm tra"
bên dưới.

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
được format trước, nên **bước Format không có trong luồng Linux** — và ô xác nhận xoá dữ
liệu chuyển sang nằm ngay ở bước ghi, vì đó mới là lúc dữ liệu biến mất.

Trên Windows, ghi thẳng ra `\\.\PHYSICALDRIVE<n>` cần ba việc đúng thứ tự: `Clear-Disk`
để hệ điều hành nhả khoá volume, đưa ổ về offline để Windows không gắn phân vùng mới vào
giữa chừng, rồi ghi theo bội số sector (khối cuối được đệm 0 cho tròn).

### Bỏ qua màn hình cài đặt ban đầu

Windows Setup tự tìm file `autounattend.xml` ở thư mục gốc của thiết bị rời. Ứng dụng sinh
file này theo cấu hình bạn chọn: ngôn ngữ, múi giờ, tên máy, tài khoản cục bộ, và tuỳ chọn
bỏ qua kiểm tra TPM. Kết quả là toàn bộ chuỗi màn hình hỏi đáp sau khi cài (vùng, bàn phím,
mạng, giấy phép, tài khoản Microsoft, quyền riêng tư) được trả lời sẵn.

File này chủ ý **không** chứa `DiskConfiguration`. Thêm vào thì Setup sẽ tự xoá và chia lại
ổ cứng đích mà không hỏi lại lần nào — quá nguy hiểm cho một công cụ mà người dùng có thể
cắm nhầm máy. Có hẳn một test khoá điều này lại để lần mở rộng sau không vô tình thêm vào.

---

## Cấu trúc

```
src-tauri/src/
  ps.rs         Cầu nối PowerShell (EncodedCommand, đọc stdout theo dòng, kiểm tra quyền admin)
  usb.rs        Nhận diện ổ USB + vòng theo dõi cắm/rút
  hardware.rs   Quét CPU/RAM/TPM/Secure Boot/firmware
  cpu.rs        Suy luận CPU có nằm trong danh sách hỗ trợ Windows 11 không
  catalog.rs    Danh mục các bản Windows + tính vòng đời theo ngày hiện tại
  distro.rs     Danh mục hệ điều hành mã nguồn mở + engine chấm điểm theo RAM
  catalog_sync.rs  Đọc bảng vòng đời từ trang release-health của Microsoft
  checks.rs     Đối chiếu 13 thành phần phần cứng với yêu cầu Windows 11
  recommend.rs  Engine chấm điểm và xếp hạng
  download.rs   Lấy link chính thức + tải có tiến trình, có resume, tính SHA-256
  writer.rs     Format ổ, chép file, tách install.wim, ghi bootsect, ghi nguyên khối
  verify.rs     Kiểm tra USB sau khi ghi: cấu trúc khởi động + đọc lại đối chiếu
  unattend.rs   Sinh autounattend.xml để bỏ qua màn hình cài đặt ban đầu
  lib.rs        Các lệnh Tauri và vòng theo dõi nền

src/
  App.tsx           Máy trạng thái theo tên bước; luồng Windows 8 bước, Linux 7
  types.ts          Khớp 1-1 với struct Rust
  lib/api.ts        Bọc invoke + listen
  components/       Titlebar, các màn hình bước, các mảnh dùng chung
  styles.css        Hệ thống thiết kế (biến CSS, sáng/tối)
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

Chạy test cho phần logic (bộ nhận diện CPU và engine gợi ý):

```bash
cd src-tauri && cargo test
```

---

## Trạng thái hiện tại

Đã kiểm chứng:

- 92 test đơn vị cho `cpu.rs`, `catalog.rs`, `catalog_sync.rs`, `checks.rs`, `recommend.rs`,
  `distro.rs`, `download.rs`, `unattend.rs`, `verify.rs`, `writer.rs` — chạy xanh
- Sáu địa chỉ `SHA256SUMS` trong danh mục distro đã kiểm chứng giải ra đúng file ISO
  hiện hành
- Toàn bộ file Rust qua được kiểm tra cú pháp
- Frontend TypeScript qua `tsc` ở chế độ `strict` không lỗi

Chưa kiểm chứng được (môi trường dựng ứng dụng không có Windows và không tải được thư viện):

- Biên dịch trọn vẹn phần Rust có phụ thuộc `tauri`/`reqwest` — hãy chạy `cargo check` lần
  đầu trên máy Windows
- Các đoạn PowerShell chạy trên máy thật, **kể cả đoạn ghi nguyên khối và đoạn đọc lại
  `\\.\PHYSICALDRIVE<n>`** — hãy thử trên một ổ USB không chứa dữ liệu quan trọng
- Luồng lấy link tải tự động của Microsoft. Đây là phần dễ hỏng nhất vì Microsoft có thể đổi
  bất cứ lúc nào, nên giao diện luôn có sẵn hai đường lui: chọn file ISO có sẵn, và mở trang
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
