#! /bin/bash

successed=$1
username=$2
passwd=$3

update_pam=/usr/share/nfs-domain-client/update_pam.sh
login_user=/usr/share/nfs-domain-client/user_login.sh
logfile=/var/log/nfs-domain-client/post_add_domain.log

if [ ! -f $logfile ]; then 
    touch $logfile
    chmod 755 $logfile
fi

print_log() {
	content=$1
	echo $(date "+%Y-%m-%d %H:%M:%S") $content >> $logfile
}

print_log "post_add_doamin args successed($successed), username($username)"

MASTER_PUB=/etc/salt/pki/minion/minion_master.pub
MASTER_PUB2=/var/lib/salt/pki/minion/minion_master.pub
MASTER_CACHE=/var/cache/salt

if [ $successed -eq 0 ]; then
	# 停止服务
	systemctl stop salt-minion
	systemctl disable salt-minion
	# 清理master公钥
	if [ -f ${MASTER_PUB} ]; then
		rm ${MASTER_PUB}
	fi
	if [ -f ${MASTER_PUB2} ]; then
		rm ${MASTER_PUB2}
	fi
	if [ -f ${MASTER_CACHE} ]; then
		rm ${MASTER_CACHE} -rf
	fi

	ipa-client-install --uninstall --unattended

	#未加域情况，停用NS中sss, 停用sssd服务
	echo "disable sssd.service" >> $logfile
	systemctl disable sssd.service

	num=`grep -c "sudoers:.*sss" /etc/nsswitch.conf `
	if [ $num -ne 0 ]; then  
		echo "disable sudoers: sss in nsswitch.conf" >> $logfile
		sed -i "/^sudoers/ s/^\(.*\)$/#\1/g" /etc/nsswitch.conf
	fi
else
	# update realm cache
	systemctl restart realmd >> $logfile

	# add pam setting
	$update_pam

	# backup sssd.conf
	cp /etc/sssd/sssd.conf /usr/share/nfs-domain-client/sssd.conf
	chattr +i /usr/share/nfs-domain-client/sssd.conf
	print_log "set chattr +i"
	print_log `cat /usr/share/nfs-domain-client/sssd.conf`

	# 清除网络设置resolv.conf编辑限制
	sed -i "/dns=none/d" /etc/NetworkManager/NetworkManager.conf

	# /etc/hosts中增加本机主机名
	line=`grep -n "^127.0.1.1 " /etc/hosts | head -n 1 |awk -F: '{print $1}'`
	if [ ! -z ${line} ]; then
		sed -i "/^127.0.1.1/d" /etc/hosts
	fi
	hostname=$(cat /usr/share/nfs-domain-client/salt/config.ini | grep "^hostname=" | awk -F '=' '{print $2}')
	sed -i "${line}i 127.0.1.1 ${hostname}" /etc/hosts

	# restart salt for server to get info
	systemctl restart salt-minion.service
       	systemctl enable salt-minion.service
	print_log "enabled salt-minion.service"

	# 登录域用户执行完才能reboot
	print_log "try to login $username"
	/usr/bin/expect $login_user $username $passwd
	print_log "login $username result: $?"
	homedir=/home/$username
	if [ -d $homedir ]; then
		chown $username:$username -R $homedir
		print_log "chown $username:$username -R $homedir result: $?"
	else
		print_log "inexistent homdir ${homedir}"
	fi

	# call CacheUser
	dbus-send --system --dest=org.freedesktop.Accounts --type=method_call \
                --print-reply /org/freedesktop/Accounts \
                org.freedesktop.Accounts.CacheUser string:$username >> $logfile	

	# 自启动
	systemctl enable nfsdomain.service
	print_log "enabled nfsdomain.service"
	#加域后 设置本地用户user密码立即过期
	# chage -d 0 user

	print_log "post_add_domain over!"
fi

exit 0
