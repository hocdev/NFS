# ISOLite 0.3 — Rust + Slint 1.18

Giao diện giữ Slint **1.18.1**. Ngoài Slint và các phụ thuộc bắt buộc của Slint, backend chỉ dùng thư viện chuẩn Rust: đọc/ghi ISO9660, CRC32/CRC64, MD5, XZ/LZMA2 và SquashFS. Không gọi 7-Zip, xorriso, mksquashfs, WSL hoặc DLL nén ở máy Windows. Các công cụ độc lập chỉ được dùng trong quá trình phát triển để đối chiếu kết quả.

Trình chỉnh sửa CachyOS Hello, callback, trình tải nguồn và backend phụ thuộc backhand/liblzma đã được loại bỏ. Slint vẫn có cây phụ thuộc của riêng nó; đây không phải dự án không có bất kỳ crate nào.

## Nhận diện tự động

- Tìm root SquashFS từ cấu trúc ISO và magic, không dựa vào tên tệp ISO. Nếu có nhiều root filesystem, dừng thay vì tự chọn sai.
- Đọc `etc/os-release`, tự giải symlink đến `usr/lib/os-release`; dùng `PRETTY_NAME`, `NAME`, `ID`. Khi thiếu metadata, dùng `.disk/info` hoặc tên ISO, không gán tên Ubuntu/Debian/NFS giả định.
- Đọc dpkg status, file ownership và desktop entries để tìm ứng dụng. Bảo vệ gói Essential/Protected, priority required/important, chủ sở hữu dịch vụ, phiên desktop, boot, bộ cài và phụ thuộc của chúng. Kiểm tra ngược Depends/Pre-Depends, alternatives và Provides trước khi gỡ.
- Danh sách locale lấy từ `usr/share/i18n/locales` trong ảnh đang mở; không gán danh sách gói hay ngôn ngữ riêng cho NFS.
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

Trong Tùy chọn, chọn locale mặc định từ danh sách tự phát hiện, hoặc nhập locale bổ sung, phân cách bằng dấu phẩy. Ví dụ `vi_VN.UTF-8,en_US.UTF-8`.

- Giữ các locale cũ, bổ sung vào `locale.gen`.
- Cập nhật LANG/LANGUAGE và gỡ LC_* mâu thuẫn trong cấu hình môi trường, giữ các khóa khác.
- Cập nhật tham số locale trên các dòng boot Live/OnlyInstall thực tế trong GRUB/ISOLINUX.
- Dịch vụ first boot chạy `localedef` có sẵn **trong Linux guest**, trước display manager. Không chạy mã Linux trên Windows.
- Có thể thêm gói dịch `.deb` đúng bản phân phối, kiến trúc và đủ phụ thuộc. Gói được giữ nguyên và cài bằng `dpkg` trong guest để bảo toàn maintainer scripts, symlink, quyền và database. Không tự lấy gói Ubuntu hoặc thay APT sources. Nếu cài/generate thất bại, dịch vụ không ghi marker thành công; xem `journalctl -u isolite-locales`.
- ISO này đã có định nghĩa locale và nhiều bản dịch tiếng Việt. Chọn locale không tự tạo bản dịch còn thiếu trong phần mềm riêng của hãng; có thể cần gói dịch bổ sung. Trình cài tùy biến/OOBE của hãng cũng cần kiểm thử thực tế.

## SquashFS và an toàn

Đọc thông số từ superblock, giữ compressor, block size/log, flags, ID table, fragment count, phiên bản và timestamp gốc. Dữ liệu nén của tệp không đổi được tái sử dụng nguyên byte, bố trí lại và cập nhật offset khi đóng gói. Chỉ tệp mới/sửa và các bảng metadata được nén lại bằng XZ/LZMA2 tự triển khai. Giữ quyền, UID/GID, symlink, hard link và xattr; tái tạo bảng directory, inode, fragment và export. Gỡ ứng dụng bỏ dữ liệu không còn được dùng; không xóa thư mục chia sẻ của gói còn lại.

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

Tham chiếu định dạng: [SquashFS trong Linux](https://github.com/torvalds/linux/blob/master/fs/squashfs/squashfs_fs.h), [đặc tả XZ](https://tukaani.org/xz/xz-file-format.txt).
