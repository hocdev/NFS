#! /bin/bash

# 用于执行同步策略

POLICYTYPE=$1
POLICY=$2
UUID_PATH=/usr/share/nfs-domain-client/salt/uuid.txt

case $POLICYTYPE in
	ssh)
		if [ $POLICY -eq 1 ]
		then
			systemctl start ssh
		else
			systemctl stop ssh
		fi
		;;
	usb)
		if [ $POLICY -eq 1 ]
		then
			/usr/share/nfs-domain-client/usb/usbpolicy 0
		else
			/usr/share/nfs-domain-client/usb/usbpolicy 1
		fi
		cat /usr/share/nfs-device-disable/status | grep "Usb" | awk '{print substr($0,length($0)-0)}' > /usr/share/nfs-domain-client/usb/usbstatus
		echo  $(date "+%Y-%m-%d %H:%M:%S") "sync usb $POLICY " >> /var/log/nfs-domain-client/usb.log
		;;
	usbstorage)
		if [ $POLICY -eq 1]
		then
			gsettings set org.cdos.devicecontrol.usbstorage usbstorage-status 1
		else
			gsettings set org.cdos.devicecontrol.usbstorage usbstorage-status 0
		fi

		;;
	wallpaper)
		uri="file:///usr/share/backgrounds/$POLICY"
		gsettings set org.cdos.desktop.background picture-uri $uri

		;;
	firewalld)
		token=`md5sum $UUID_PATH | awk '{print $1}'`
		echo "token=$token"
		gdbus call --system --dest=com.nfs.securitycenter --object-path=/SecuritySetting/firewall --method=com.nfs.securitycenter.firewallsetting.WakeUp
		
		if [ $POLICY -eq 1 ]
		then
			gdbus call --system --dest=com.nfs.securitycenter --object-path=/SecuritySetting/firewall --method=com.nfs.securitycenter.firewallsetting.StartFirewall $token
		else
			gdbus call --system --dest=com.nfs.securitycenter --object-path=/SecuritySetting/firewall --method=com.nfs.securitycenter.firewallsetting.CloseFirewall $token
		fi

		;;
	firewalld_level)
		token=`md5sum $UUID_PATH | awk '{print $1}'`
		echo "token=$token"
		gdbus call --system --dest=com.nfs.securitycenter --object-path=/SecuritySetting/firewall --method=com.nfs.securitycenter.firewallsetting.WakeUp
		gdbus call --system --dest=com.nfs.securitycenter --object-path=/SecuritySetting/firewall --method=com.nfs.securitycenter.firewallsetting.SettingFirewallLevel $POLICY $token
		;;
	*)
		echo "error policy type"
esac

exit 0

