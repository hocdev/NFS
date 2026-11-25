#!/bin/bash
#decrypt it first
cryptLine=$(blkid | grep crypto_LUKS)
if [ -n "$cryptLine" ];then
    password=$(kdialog --password "Encrypted disk detected, please enter password")
    cryptDisks=$(blkid | grep crypto_LUKS | awk -F: '{print $1}')
    for disk in $cryptDisks
    do
        echo $password | cryptsetup luksOpen $disk ${disk##*\/}Crypted
        result=$?
        if [ $result = 2 ];then
            exit 1
        fi
    done
fi
ARCH=$(arch)
probeResult=$(os-prober | grep nfsdesktop | grep -v backup)
if [ -z "$probeResult" ];then
	if [ -z "$1" ];then
    	echo "No Linux system found on disk"
    	exit 1
    else
    	probeResult="$1"
    fi
fi
echo os $probeResult
ospartition=$(echo $probeResult | awk -F: '{ print $1 }')
echo ospartition $ospartition
osname=$(echo $probeResult | awk -F: '{ print $3 }')
echo osname $osname
#if [ $osname != nfsdesktop ];then
#    echo "Not the Fangde system"
#fi
rootMountPoint="/tmp/root-nfs"
isEfi=false
if [ -d /sys/firmware/efi/efivars ];then
    isEfi=true
fi
isLvm=false
bootPartition=
efiPartition=$(echo $ospartition | sed "s/[0-9]$/1/g")
if [[ $ospartition = *"mapper"* ]];then
    isLvm=true
    if [ $isEfi = true ];then 
        efiPartition=/dev/sda1
        bootPartition=/dev/sda2
        if [ -e /dev/nvme0n1 ];then
            efiPartition=/dev/nvme0n1p1
            bootPartition=/dev/nvme0n1p2
        fi
    else
	bootPartition=/dev/sda1
        if [ -e /dev/nvme0n1 ];then
            bootPartition=/dev/nvme0n1p1
        fi
    fi
fi
bootMountPoint=$rootMountPoint/boot
deviceNode=$(echo $ospartition | sed "s/[0-9]$//g")
efiMountPoint=$rootMountPoint/boot/efi
#if [ $ARCH = sw_64 ];then
#    efiMountPoint=$rootMountPoint/boot
#    if [ $isLvm = true ];then
#        efiMountPoint=$rootMountPoint/boot/efi
#    fi	
#fi
mountSystem () {
    mkdir -p $rootMountPoint
    mount $ospartition $rootMountPoint || echo "mount rootfs failed"
    mkdir -p $rootMountPoint/proc
    mount -vt proc proc $rootMountPoint/proc || echo "mount proc failed"
    mkdir -p $rootMountPoint/sys
    mount -vt sysfs sys $rootMountPoint/sys || echo "mount sys failed"
    mkdir -p $rootMountPoint/run
    mount -vt tmpfs tmpfs $rootMountPoint/run || echo "mount tmpfs failed"
    mkdir -p $rootMountPoint/dev
    mount --bind /dev $rootMountPoint/dev || echo "mount dev failed"
    mkdir -p $rootMountPoint/run/udev
    mount --bind /run/udev $rootMountPoint/run/udev || echo "mount udev failed"
    if [ true = $isLvm ];then
        mount $bootPartition $bootMountPoint
    fi
    if [ true = $isEfi ];then
        mkdir -p $rootMountPoint/sys/firmware/efi/efivars
        mount -vt efivarfs efivarfs $rootMountPoint/sys/firmware/efi/efivars
	mkdir -p $efiMountPoint
	mount $efiPartition $efiMountPoint
    fi
}
umountSystem () {
    if [ true = $isEfi ];then
        umount -lv $rootMountPoint/sys/firmware/efi/efivars
	umount -lv $rootMountPoint/boot/efi
    fi
    if [ true = $isLvm ];then
        umount -lv $bootMountPoint
    fi
    umount -lv $rootMountPoint/run/udev
    umount -lv $rootMountPoint/dev
    umount -lv $rootMountPoint/run
    umount -lv $rootMountPoint/sys
    umount -lv $rootMountPoint/proc
    umount -lv $rootMountPoint
}
mountSystem
if [[ $ARCH = x86_64 || $ARCH = AMD64 || $ARCH = amd64 || $ARCH = X86_64 || $ARCH = ARM64 || $ARCH = aarch64 || $ARCH = arm64 || $ARCH = AARCH64 || $ARCH = loongarch64 ]];then
    mkdir -p $rootMountPoint/boot/grub
    if [ $isEfi = true ];then
        mkdir -p $rootMountPoint/boot/efi
	efi_bootloader_id="NFS Desktop"
	efi_bitness=$(cat /sys/firmware/efi/fw_platform_size)
	efi_target=
	efi_grub_file=
	efi_boot_file=
	if [[ $ARCH = AMD64 || $ARCH = amd64 || $ARCH = x86_64 || $ARCH = X86_64 ]];then
	    if [ $efi_bitness = 32 ];then
                efi_target="i386-efi"
		efi_grub_file="grubia32.efi"
		efi_boot_file="bootia32.efi"
	    elif [ $efi_bitness = 64 ];then
		efi_target="x86_64-efi"
		efi_grub_file="grubx64.efi"
		efi_boot_file="bootx64.efi"
	    fi
	elif [[ $ARCH = ARM64 || $ARCH = arm64 || $ARCH = aarch64 || $ARCH = AARCH64 ]];then
	    if [ $efi_bitness = 32 ];then
                efi_target="arm-efi"
		efi_grub_file="grubaa32.efi"
		efi_boot_file="BOOTAA32.efi"
	    elif [ $efi_bitness = 64 ];then
		efi_target="arm64-efi"
		efi_grub_file="grubaa64.efi"
		efi_boot_file="BOOTAA64.EFI"
	    fi
	elif [ $ARCH = loongarch64 ];then
	    efi_target="loongarch64-efi"
	    efi_grub_file="grubloongarch64.efi"
	    efi_boot_file="BOOTLOONGARCH64.EFI"
	fi
	chroot $rootMountPoint grub-install --target=$efi_target --efi-directory=/boot/efi --bootloader-id=$efi_bootloader_id
    else
        chroot $rootMountPoint grub-install --target=i386-pc --recheck --force $deviceNode
    fi
    chroot $rootMountPoint grub-mkconfig -o /boot/grub/grub.cfg
elif [ $ARCH = sw_64 ];then
    chroot $rootMountPoint grub-mkconfig -o /boot/grub/grub.cfg
else
    mkdir -p $rootMountPoint/boot/grub
    mkdir -p $rootMountPoint/boot/efi
    chroot $rootMountPoint grub-mkconfig -o /boot/grub/grub.cfg 
    chroot $rootMountPoint cp -rf --parents /EFI/BOOT /boot/efi
    if [ -e $rootMountPoint/boot/efi/EFI/BOOT/grub.cfg ];then
        rm -f $rootMountPoint/boot/efi/EFI/BOOT/grub.cfg
    fi
    chroot $rootMountPoint cp /boot/grub/grub.cfg /boot/efi/EFI/BOOT/
fi
umountSystem
echo "Repair completed"
