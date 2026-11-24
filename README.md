# ISOLite - Công cụ tùy biến & đóng gói ISO Linux (Bản Rust + Slint 1.18)

Công cụ native x64 Windows được viết hoàn toàn 100% bằng **Rust** kết hợp giao diện đồ họa hiện đại **Slint 1.18**, hỗ trợ tùy biến các bản phân phối Linux (Ubuntu, Debian, Linux Mint, Zorin OS, Arch Linux, CachyOS...). Khởi chạy trực tiếp file thực thi duy nhất `ISOLite.exe`, **hoàn toàn không cần bất kỳ công cụ ngoài** hay thư mục phụ thuộc (`mksquashfs.exe`, `xorriso.exe`, các DLL runtime MSYS2).

---

## 🌟 Tính Năng & Kiến Trúc 100% Rust

- **100% Rust & Zero External Runtime Dependencies**:
  - Tự cài đặt và xử lý trực tiếp toàn bộ thuật toán hệ thống tập tin đĩa ISO: **ISO9660, Joliet, Rock Ridge, El Torito, isohybrid MBR, Primary & Backup GPT**.
  - Đọc, lọc bỏ và đóng gói lại ảnh nén **SquashFS** trực tiếp bằng crate `backhand` (hỗ trợ XZ, Zstd, Gzip, LZ4 đa luồng song song).
  - Phân tích và nạp trực tiếp các định dạng gói cài đặt: `.deb` (Debian/Ubuntu qua định dạng Ar archive & Tar) và `.pkg.tar.zst` / `.pkg.tar.xz` (Arch/CachyOS qua Tar & Zstd/Xz).
  - Bộ đệm nhị phân (Binary Cache Engine) định dạng `0x5A4C4339` ('ZLC9') với mã băm FNV-1a siêu tốc, giúp nạp ảnh ISO lớn hàng gigabyte chỉ trong 0.05 giây.
- **Giao diện hiện đại Slint 1.18**:
  - Thiết kế tông màu tối (Dark mode) sang trọng, chuẩn công nghiệp với thanh bên Sidebar và 4 khu vực làm việc:
    1. **Ứng dụng & Gói hệ thống**: Lọc theo danh mục (Bên thứ ba / Hệ thống), tìm kiếm tức thì theo từ khóa, hộp kiểm bật/tắt gỡ bỏ, thống kê dung lượng và xuất tệp gói.
    2. **Gói cài thêm (.deb / .pkg.tar.zst)**: Thêm gói bằng hộp thoại hoặc kéo thả, kiểm tra tính hợp lệ và tự động cấu hình cơ sở dữ liệu dpkg / pacman.
    3. **Tùy chọn & Đóng gói**: Dọn dẹp kho Flatpak (tiết kiệm hàng GB dung lượng), đổi tên bản phát hành (os-release, issue, .disk/info), thêm mục Refresh vào menu KDE Plasma 6, thanh tiến trình % trực quan và cửa sổ nhật ký (log console) thời gian thực.
    4. **Sửa CachyOS Hello**: Trình biên tập trực quan các nút bấm, biểu tượng, đường dẫn lệnh của ứng dụng CachyOS Hello (hỗ trợ tải mã nguồn Glade XML từ GitHub).
- **Hỗ trợ đầy đủ cả 2 chế độ: GUI & CLI (Command-Line Interface)**:
  - Khởi chạy không tham số: mở giao diện đồ họa Slint 1.18.
  - Chạy với tham số dòng lệnh phục vụ tự động hóa và script:
    - `isolite inspect <input.iso>`: Quét và hiển thị thông tin bản phát hành, danh sách gói.
    - `isolite extract <input.iso> <output-folder>`: Giải nén toàn bộ tệp trong ISO.
    - `isolite build <input.iso> <output.iso> [tùy chọn]`: Đóng gói tự động với các cờ `--remove <pkg>`, `--deb <file>`, `--clean-flatpak`, `--desktop-installer`, `--desktop-refresh`.

---

## 🛠 Hướng Dẫn Biên Dịch & Chạy

### Yêu Cầu Hệ Thống:
- Windows 10 / 11 64-bit.
- Rust toolchain (Rust 1.80+ hoặc bản mới nhất qua `rustup`).

### Các Lệnh Biên Dịch:
```bash
# Kiểm tra mã nguồn
cargo check

# Chạy kiểm thử tự động (Unit & Integration tests)
cargo test

# Biên dịch bản phát hành tối ưu cao nhất (LTO, strip)
cargo build --release
```
File thực thi sau khi biên dịch nằm tại: `target/release/isolite.exe` (hoặc `ISOLite.exe`).

---

## 📖 Hướng Dẫn Sử Dụng Giao Diện

1. **Mở ISO**: Bấm **📂 Mở ISO...** và chọn tệp `.iso` của Ubuntu/Debian hoặc Arch/CachyOS.
2. **Chọn gói gỡ bỏ**: Chuyển sang mục **Ứng dụng & Gói hệ thống**, tìm kiếm và đánh dấu chọn các phần mềm muốn loại bỏ để tiết kiệm dung lượng.
3. **Thêm gói cài đặt mới**: Sang mục **Gói cài thêm**, bấm **＋ Thêm gói...** và chọn các tệp `.deb` hoặc `.pkg.tar.zst` muốn tích hợp sẵn vào hệ điều hành. Hệ thống tự động cấu hình bộ gõ Tiếng Việt (Fcitx5 / Bamboo / Unikey) nếu có gói tương ứng.
4. **Tùy chọn đóng gói**: Sang mục **Tùy chọn & Đóng gói**, tích chọn **Dọn dẹp Flatpak**, tùy biến tên hệ điều hành nếu muốn, rồi bấm **▶ BẮT ĐẦU TẠO ISO** để tạo bản ISO bootable mới.
