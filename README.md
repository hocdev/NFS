# ISOLite 0.4 — Rust + Slint 1.18

Giao diện giữ Slint **1.18.1**. Ngoài Slint và các phụ thuộc bắt buộc của Slint, backend chỉ dùng thư viện chuẩn Rust: đọc/ghi ISO9660, CRC32/CRC64, MD5, XZ/LZMA2 và SquashFS. Không gọi 7-Zip, xorriso, mksquashfs, WSL hoặc DLL nén ở máy Windows. Các công cụ độc lập chỉ được dùng trong quá trình phát triển để đối chiếu kết quả.

Trình chỉnh sửa CachyOS Hello, callback, trình tải nguồn và backend phụ thuộc backhand/liblzma đã được loại bỏ. Slint vẫn có cây phụ thuộc của riêng nó; đây không phải dự án không có bất kỳ crate nào.

## Nhận diện tự động

- Tìm root SquashFS từ cấu trúc ISO và magic, không dựa vào tên tệp ISO. Nếu có nhiều root filesystem, dừng thay vì tự chọn sai.
- Đọc `etc/os-release`, tự giải symlink đến `usr/lib/os-release`; dùng `PRETTY_NAME`, `NAME`, `ID`. Khi thiếu metadata, dùng `.disk/info` hoặc tên ISO, không gán tên Ubuntu/Debian/NFS giả định.
- Đọc dpkg status, file ownership và desktop entries để tìm ứng dụng. Bảo vệ gói Essential/Protected, priority required/important, chủ sở hữu dịch vụ, phiên desktop, boot, bộ cài và phụ thuộc của chúng. Kiểm tra ngược Depends/Pre-Depends, alternatives và Provides trước khi gỡ.
- Tách rõ ngôn ngữ đã có (đọc locale-archive và dữ liệu LC_*) với 302 locale UTF-8 có thể thêm từ bộ dữ liệu glibc 2.31. Danh mục sinh từ dữ liệu thực tế, có tên dễ đọc và ô tích chọn; ngôn ngữ đã có được bỏ qua.
- Backend hiện hỗ trợ root SquashFS **4.0 little endian, XZ/LZMA2, dpkg**. ISO dùng bộ nén khác hoặc database khác sẽ báo chưa hỗ trợ.

## ISO đã kiểm tra

`NFSDesktop-5.0-G240-251209-amd64.iso` chứa:

```text
PRETTY_NAME="NFSChina Desktop OS V5.0-G240"
ID=nfsdesktop
ID_LIKE=debian
VERSION_CODENAME=tiger
/etc/debian_version: 11.3
kernel: 5.4.0-100-generic
root: /live/filesystem.squashfs
SquashFS: 4.0, XZ, block 131072 bytes
```

Trong VMware chọn **Linux → Debian 11.x 64-bit** theo nền Debian 11 được xác nhận từ ISO. Tên máy ảo “Ubuntu 64-bit” trong ảnh chỉ là cấu hình guest đã chọn, không xác định nền của ISO.

## Thêm ngôn ngữ

Mở ISO, vào mục **Ngôn ngữ** ở thanh bên:

1. Các mục **Có sẵn** đã có dữ liệu locale trong ISO, không thêm trùng.
2. Tích Tiếng Việt, French, Japanese hoặc các ngôn ngữ khác muốn thêm. Bộ glibc 2.31 đi kèm có **302 locale UTF-8**. Có tìm kiếm theo tên hoặc mã; không cần nhập mã locale hoặc chọn `.deb`.
3. **Giao diện mặc định của ISO** đặt ngôn ngữ trước khi đóng gói. Bạn chọn **Tiếng Anh (Hoa Kỳ)** trong danh sách để đổi từ tiếng Trung; chương trình không ép ngôn ngữ mặc định bằng mã. ISO mới khởi động thẳng bằng ngôn ngữ đã chọn thay cho tiếng Trung; không thêm màn hình lựa chọn lúc boot.

Dữ liệu LC_* đã biên dịch, bản dịch có sẵn và LANG/LANGUAGE được tích hợp **trực tiếp vào SquashFS**. Không có `setup-locales.sh`, service cài ngôn ngữ, `localedef`, `locale-gen` hoặc `dpkg` do ISOLite thêm để chạy khi boot. Thành phần locales nguyên bản của Debian Live dùng `--keep-existing` sẽ bỏ qua locale đã sẵn sàng.

- Tiếng Anh có trong chính locale-archive của NFS; được giữ nguyên.
- Đã tải `pkg/languages/locales-all_2.31-13+deb11u11_amd64.deb` từ kho Debian 11 chính thức. Kho NFS trong ISO không phản hồi khi kiểm tra. Không gọi đây là gói chính thức do NFS phát hành.
- Dữ liệu được nhúng trong EXE, chọn theo ABI glibc đọc từ dpkg. Không cài hoặc thay libc. Có kiểm tra category magic/định dạng trước khi áp dụng; không áp dữ liệu 2.31 sang glibc khác.
- `pkg/languages/` là thư mục dữ liệu ngôn ngữ duy nhất, chứa gói tải sẵn, catalog tự động, nguồn và bản quyền; không có bundle hay nhánh xử lý riêng cho Việt/Anh. Các gói trong `pkg/languages/` được dò tự động; ABI và kiến trúc lấy từ control metadata. Catalog được đọc từ payload LC_CTYPE; sidecar `.catalog.tsv` chỉ dùng nếu CRC gói khớp. Thêm/thay gói dữ liệu không cần sửa tên file hoặc nhánh ngôn ngữ trong Rust. Registry nhúng được sinh trong OUT_DIR, không có nhánh đặc biệt Việt/Anh.
- Locale không tự tạo bản dịch giao diện còn thiếu trong phần mềm riêng của hãng. Nhiều bản dịch GNU/GNOME/Qt đã có trong ISO; phần mềm chỉ có tiếng Trung vẫn cần bản dịch tương ứng. CLI `--language-deb` chỉ trích xuất dữ liệu dịch hợp lệ, không chạy maintainer script.
- Gói ứng dụng ở mục **Gói cài thêm** là chức năng riêng, không được dùng để triển khai ngôn ngữ.

## Tốc độ và giao diện

Đọc file ownership/desktop qua chỉ mục SquashFS thay vì duyệt lại thư mục cho từng gói; tránh sao chép block metadata khi cache đã có. Cache nhị phân danh sách dùng chữ ký đường dẫn, kích thước, mtime và CRC mẫu ISO; có CRC payload và giới hạn kích thước. Cache chỉ phục vụ giao diện: khi build vẫn đọc lại nguồn và kiểm tra phụ thuộc.

Đo cùng ISO: trước 6,20 s; sau tối ưu lần đầu 2,08 s; cache khoảng 0,01 s. Thời gian phụ thuộc ổ đĩa và máy. Widget dùng bảng màu tối, mục ngôn ngữ có lựa chọn rõ ràng thay cho ô nhập locale.

`build.rs` ở gốc đã xóa. Cargo tự chạy `build-support/main.rs`; icon ICO được chuyển thành tài nguyên COFF bằng Rust chuẩn, nhúng vào PE trong quá trình link, không cần winres/windres.

## SquashFS và an toàn

Đọc thông số từ superblock, giữ compressor, block size/log, flags, ID table, fragment count, phiên bản và timestamp gốc. Dữ liệu nén của tệp không đổi được tái sử dụng nguyên byte, bố trí lại và cập nhật offset khi đóng gói. Chỉ tệp mới/sửa và các bảng metadata được nén lại bằng XZ/LZMA2 tự triển khai. Giữ quyền, UID/GID, symlink, hard link và xattr; tái tạo bảng directory, inode, fragment và export. Gỡ ứng dụng bỏ dữ liệu nguyên block không còn được dùng; không xóa thư mục chia sẻ của gói còn lại. Các block fragment và ID của chúng được giữ để bảo toàn đuôi tệp dùng chung; dung lượng giảm có thể thấp hơn Installed-Size.

“Giống gốc” nghĩa là giữ định dạng/thông số và metadata có ý nghĩa, **không phải cùng hash hoặc byte-for-byte**. Thuật toán chọn match của encoder có thể tạo tỷ lệ nén/kích thước khác mksquashfs gốc. ISO có thể cần mở rộng để chứa nội dung mới.

ISO nguồn chỉ mở để đọc; từ chối đích đã tồn tại. Ghi vào thư mục staging, chỉ công bố tệp đích sau khi:

1. Đọc lại SquashFS, đối chiếu tham số, toàn bộ đường dẫn, metadata, nội dung tệp thêm/sửa và tệp đã gỡ.
2. Đối chiếu byte nén của dữ liệu/fragment không sửa với nguồn.
3. Cập nhật manifest, installed size và các danh sách MD5 hiện có.
4. Đối chiếu danh sách/nội dung mọi tệp ISO với nguồn hoặc tệp thay thế, cùng cấu trúc boot El Torito/hybrid. EFI image và boot catalog không bị sửa nội dung.

NFS có chữ ký riêng `/.md5sum.txt`. Thay nội dung làm chữ ký nhà phát hành không còn hợp lệ; không có khóa riêng để ký lại. Không xóa hoặc giả chữ ký. Kiểm tra cấu trúc không thay thế được kiểm thử boot và cài đặt BIOS/UEFI trong máy ảo; chưa thể cam kết bộ cài NFS chấp nhận ISO tùy biến.

## Biên dịch

Cần Rust và linker tương ứng. MSVC cần Visual C++ Build Tools; GNU cần GCC/MinGW tương thích. Slint build-time là ngoại lệ đã được cho phép.

```powershell
cargo check
cargo test
cargo build --release
```

```powershell
.\target\release\isolite.exe inspect .\input.iso
.\target\release\isolite.exe build .\input.iso .\output.iso --default-locale vi_VN.UTF-8 --locale en_US.UTF-8
.\target\release\isolite.exe build .\input.iso .\output.iso --remove ten-goi --language-deb .\goi-dich.deb
```

Tham chiếu định dạng: [SquashFS trong Linux](https://github.com/torvalds/linux/blob/master/fs/squashfs/squashfs_fs.h), [đặc tả XZ](https://tukaani.org/xz/xz-file-format.txt), [systemd unit/drop-in](https://www.freedesktop.org/software/systemd/man/latest/systemd.unit.html).
