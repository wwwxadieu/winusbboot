# Get WinUSB

Ứng dụng Windows tự nhận diện ổ USB đang cắm, quét cấu hình máy, gợi ý phiên bản
Windows phù hợp nhất rồi tạo luôn USB cài đặt.

Tauri 2 + React 18 + TypeScript. Backend Rust, giao diện kính xếp lớp có chế độ sáng/tối.

---

## Ứng dụng làm gì

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
  catalog_sync.rs  Đọc bảng vòng đời từ trang release-health của Microsoft
  checks.rs     Đối chiếu 13 thành phần phần cứng với yêu cầu Windows 11
  recommend.rs  Engine chấm điểm và xếp hạng
  download.rs   Lấy link chính thức + tải có tiến trình, có resume, tính SHA-256
  writer.rs     Format ổ, chép file, tách install.wim, ghi bootsect
  unattend.rs   Sinh autounattend.xml để bỏ qua màn hình cài đặt ban đầu
  lib.rs        Các lệnh Tauri và vòng theo dõi nền

src/
  App.tsx           Máy trạng thái 6 bước
  types.ts          Khớp 1-1 với struct Rust
  lib/api.ts        Bọc invoke + listen
  components/       Titlebar, 6 màn hình bước, các mảnh dùng chung
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

- 40 test đơn vị cho `cpu.rs`, `catalog.rs`, `catalog_sync.rs`, `checks.rs`, `recommend.rs`,
  `writer.rs` — chạy xanh
- Toàn bộ file Rust qua được kiểm tra cú pháp
- Frontend TypeScript qua `tsc` ở chế độ `strict` không lỗi

Chưa kiểm chứng được (môi trường dựng ứng dụng không có Windows và không tải được thư viện):

- Biên dịch trọn vẹn phần Rust có phụ thuộc `tauri`/`reqwest` — hãy chạy `cargo check` lần
  đầu trên máy Windows
- Các đoạn PowerShell chạy trên máy thật
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
