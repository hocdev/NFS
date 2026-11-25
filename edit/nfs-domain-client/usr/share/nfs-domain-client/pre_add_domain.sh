#!/bin/bash
# 仅仅在nfsdesktop 5.0系统下执行

PATH=/bin:/sbin:/usr/bin:/usr/sbin:/usr/local/bin:/usr/local/sbin:~/bin
export PATH
LANG=en_US.UTF-8
LOG_DIR=/var/log/nfs-domain-client
LOG_FILE="$LOG_DIR/pre_add_domain.log"
LOG_INFO="Info"
LOG_ERROR="Error"
SERVER_IP=$1
DOMAIN_NAME=$2
domain_config=/usr/share/nfs-domain-client/salt/config.ini
repo_source=/etc/apt/sources.list.d/${SERVER_IP}.list

OSType=$(cat /etc/os-release | grep "^ID=" | awk -F '=' '{print $2}')
OSVersionId=$(cat /etc/os-release | grep "^VERSION_ID=" | awk -F '=' '{print $2}')

if [ ! -f $LOG_FILE ]; then 
    touch $LOG_FILE
    chmod 755 $LOG_FILE
fi

print_log()
{
	log_level=$1
	info=$2
	now=`date "+%Y-%m-%d %H:%M:%S"`
	echo "$now [$log_level]: $info" >> $LOG_FILE
}

print_log $LOG_INFO "Server IP: $SERVER_IP, Domain: $DOMAIN_NAME, OS: ${OSType}, Version: ${OSVersionId}"

if [ "${OSType}" != 'nfsdesktop' ] || [ "${OSVersionId}" != '"5.0"' ]; then
	print_log $LOG_ERROR "Error: must be nfsdesktop 5.0 OS"
#	exit 1
fi

Set_HOSTS(){
	hostcheck=$(cat /etc/hosts|grep -c "^${SERVER_IP} ${DOMAIN_NAME}")
	if [ "${hostcheck}" == '1' ];then
	    print_log $LOG_INFO "do nothing for /etc/host"
		return
	fi

	# 重新写入IP和域名信息
	sed -i "/${SERVER_IP}/d" /etc/hosts
	sed -i "/${DOMAIN_NAME}/d" /etc/hosts
	echo "${SERVER_IP} ${DOMAIN_NAME}" >> /etc/hosts

	domain2="${DOMAIN_NAME#server.}"
	echo "${SERVER_IP} ${domain2}" >> /etc/hosts
}

Set_DNS(){
	# sed -i "/^\[main\]/a\dns=none" /etc/NetworkManager/NetworkManager.conf	
	# nmcli general reload

	sed -i "/${SERVER_IP}/d" /etc/resolv.conf
	nameserver_line=`grep -n "^nameserver " /etc/resolv.conf | head -n 1 |awk -F: '{print $1}'`
	local line=1 # no search no nameserver
	if [ -z $nameserver_line ]; then
		search_line=`grep -n "^search " /etc/resolv.conf | tail -n 1 |awk -F: '{print $1}'`
		if [ ! -z $search_line ]; then
			line=`expr $search_line + 1`
		fi
	else
		line=${nameserver_line}
	fi
	print_log $LOG_INFO "/etc/resolv.conf: insert line(${line})"
	sed -i "${line}i\nameserver ${SERVER_IP}" /etc/resolv.conf
}
add_patch_source(){
  if [ "${OSType}" == 'nfsdesktop' ];then
    if [ "${OSVersionId}" == '"5.0"' ];then
      echo "deb [trusted=yes] http://${SERVER_IP}/aptly/ 5.0 security sys" > /etc/apt/sources.list.d/nfsupdate.list
	#    echo "deb [trusted=yes] http://${SERVER_IP}:9778/ 5.0 security sys" > /etc/apt/sources.list.d/nfsupdate.list
	  print_log $LOG_INFO "add patch source nfsupdate.list"
	fi
  fi
}
Set_RepRepro() {
	# Register apt repository
	eval $(apt-config shell APT_SOURCE_PARTS Dir::Etc::sourceparts/d)
	CODE_SOURCE_PART=${APT_SOURCE_PARTS}${SERVER_IP}.list

	eval $(apt-config shell APT_TRUSTED_PARTS Dir::Etc::trustedparts/d)
	CODE_TRUSTED_PART=${APT_TRUSTED_PARTS}${SERVER_IP}.gpg

	if [ -f $CODE_SOURCE_PART ]; then
		rm -f $CODE_SOURCE_PART
	fi

	# bak all list	
	# mv /etc/apt/sources.list /etc/apt/sources.list.bak

	if [ "${OSType}" == 'nfsdesktop' ];then
		if [ -f "$repo_source" ]; then
			rm "$repo_source"
		fi

		if [ "${OSVersionId}" == '"5.0"' ];then
			# 最新G212下，不能屏蔽系统源，部份依赖包更新需要依赖系统源
			if [ "$(uname -m)" == "aarch64" ];then
				echo "deb [trusted=yes] http://${SERVER_IP}/aptly/nfs5-arm nfs5-arm main" > /etc/apt/sources.list.d/${SERVER_IP}.list
			else
				echo "deb [trusted=yes] http://${SERVER_IP}/aptly/server server main" > /etc/apt/sources.list.d/${SERVER_IP}.list
			fi
		elif [ "${OSVersionId}" = '4.0.0' -o "${OSVersionId}" = '4.0' ];then
			for file in $sources_dir/*; do
				if [ "$file" != "/etc/apt/sources.list.d/cdos-appstore.list" ]; then
					mv "$file" "${file}.bak"
				else
					# add cdos-appstore.list #
					sed -i 's/^/#/' /etc/apt/sources.list.d/cdos-appstore.list
				fi
			done

			echo "deb [trusted=yes] http://${SERVER_IP}:9778/nfs4 nfs4 main" > $repo_source
		fi
	elif [ "${OSType}" == 'kylin' ]; then
		if [ "${KylinPrettyName}" = '"Kylin V10 SP1"' ];then
			mv $sources_dir /etc/apt/sources.list.d.bak
			mkdir  $sources_dir
			echo "deb [trusted=yes] http://${SERVER_IP}:9778/kylin-v10sp1 kylin-v10sp1 main" > $repo_source
		elif [ "${KylinPrettyName}" = '"Kylin V10"' ];then
			mv $sources_dir /etc/apt/sources.list.d.bak
			mkdir  $sources_dir
			echo "deb [trusted=yes] http://${SERVER_IP}:9778/kylin-v10 kylin-v10 main" > $repo_source
		else
			echo ${KylinPrettyName}
		fi			
	elif [ "${OSType}" == 'uos' ]; then
		mv /etc/apt/sources.list.d /etc/apt/sources.list.d.bak
		mkdir  /etc/apt/sources.list.d
		echo "deb [trusted=yes] http://${SERVER_IP}:9778/uos-1060 uos-1060 main" > $repo_source
	fi
	
	if [ -f $CODE_TRUSTED_PART ]; then
		rm -f $CODE_TRUSTED_PART
	fi
	wget -qO - http://${SERVER_IP}:9778/nfs-domain.gpg.key | apt-key add -
}
 

Install_Deb_Pack(){
		
	Set_RepRepro

	# 使用系统自带的 systemd-timesyncd ,不再使用chrony, 但需要配置systemd-timesync，并在ipa加域命令中增加 --no-ntp
	if dpkg -l | grep -q "^ii.* systemd-timesyncd " ; then
		cat <<EOF > /etc/systemd/timesyncd.conf
[Time]
NTP=10.10.10.241 $DOMAIN_NAME
RootDistanceMaxSec=5
PollIntervalMinSec=32
PollIntervalMaxSec=2048
EOF
		systemctl restart systemd-timesyncd.service
		print_log $LOG_INFO "restart systemd-timesyncd service"
	fi

	#准备加域，启用NS中sss, 启用sssd服务
	# if [ ! -f "/etc/sssd/sssd.conf" ]; then
		print_log $LOG_INFO "enable sssd.service"
		systemctl enable sssd.service

		num=`grep -c "#sudoers:.*sss" /etc/nsswitch.conf `
		if [ $num -ne 0 ]; then  
			print_log $LOG_INFO "enable sudoers: sss in nsswitch.conf"
			sed -i "s/^#sudoers/sudoers/" /etc/nsswitch.conf
		fi
	# fi

	# 更新config.ini配置ip_port字段
	num=`grep -c "^add_domain_success=true" $domain_config`
	if [ $num -eq 0 ]; then
		# 更新ip_port
		ip_port_line=`grep -n "^ip_port=" $domain_config |awk -F: '{print $1}'`
		sed -i "${ip_port_line}"'d' $domain_config
		sed -i "${ip_port_line}i\ip_port=$SERVER_IP:80" $domain_config

		# 更新domain_name
		domain_name_line=`grep -n "^domain_name=" $domain_config | awk -F: '{print $1}'`
		if [ -n "$domain_name_line" ]; then
			sed -i "${domain_name_line}"'d' $domain_config
		else
			domain_name_line=`expr $ip_port_line + 1`
		fi
		sed -i "${domain_name_line}i\domain_name=$DOMAIN_NAME" $domain_config
	fi
}

clear_ipa_sssdtools(){
	
	#卸载sssd-tools 避免影响adduser执行时出现sssd cache相关错误信息。
	# 检查 sssd-tools 是否已安装
	if dpkg -l | grep -q "^ii  sssd-tools "; then
		print_log $LOG_INFO "sssd-tools is installed. Uninstalling..."
		sudo apt-get remove -y sssd-tools
		if [ $? -eq 0 ]; then
			print_log $LOG_INFO "sssd-tools has been successfully uninstalled."
		else
			print_log $LOG_INFO "Failed to uninstall sssd-tools."
		fi
	else
		print_log $LOG_INFO "sssd-tools is not installed."
	fi
	# freeipa退域 针对退域时卸载不完全的问题
	print_log $LOG_INFO "run ipa-client-install --uninstall  --unattended"
	ipa-client-install --uninstall  --unattended
}

write_domain_preferences(){
	# 定义文件路径
	PREFERENCES_FILE="/etc/apt/preferences.d/1001nfsdomain"	

	# 如果文件存在，删除它
	if [ -f "$PREFERENCES_FILE" ]; then
		rm "$PREFERENCES_FILE"
		print_log $LOG_INFO "deleted file : $PREFERENCES_FILE"
	fi
	
	# 写入内容到文件，使用 sudo 和 tee
	echo "Package: *" | sudo tee $PREFERENCES_FILE > /dev/null
	echo "Pin: origin \"$SERVER_IP\"" | sudo tee -a $PREFERENCES_FILE > /dev/null
	echo "Pin-Priority: 1001" | sudo tee -a $PREFERENCES_FILE > /dev/null

	print_log $LOG_INFO "write_domain_preferences $PREFERENCES_FILE"
}


domain_type=$(grep "domain_type" $domain_config | cut -d "=" -f2)
print_log $LOG_INFO "domain_type is  $domain_type"

startTime=`date +%s`
Set_DNS
Set_HOSTS
Install_Deb_Pack
if [ "$domain_type" -eq 3 ]; then
	print_log $LOG_INFO "auto join domain ,not set nfsupdate.list"
else
	print_log $LOG_INFO " set nfsupdate.list"
	add_patch_source
fi
# add_patch_source 系统镜像制作替代shell更改
write_domain_preferences

clear_ipa_sssdtools

endTime=`date +%s`
((outTime=$endTime-$startTime))
print_log $LOG_INFO "$(echo -e "Time consumed:\033[32m $outTime \033[0mSeconds!")"

exit 0
