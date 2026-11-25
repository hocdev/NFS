#!/bin/bash
PATH=/bin:/sbin:/usr/bin:/usr/sbin:/usr/local/bin:/usr/local/sbin:~/bin
export PATH
LANG=en_US.UTF-8
python_bin=/usr/bin/python

domain_config=/usr/share/nfs-domain-client/salt/config.ini
sources_dir=/etc/apt/sources.list.d

SERVER_IP=$1
DOMAIN_NAME=$2
DOMAIN_TYPE=$3
server_source=/etc/apt/sources.list.d/${SERVER_IP}.list

if [[ ! ${DOMAIN_TYPE} =~ ^[0-9]+$ ]]; then
	# 默认为管控模式
	DOMAIN_TYPE=2	
fi

INSTALL_DEPEND_LOG=/var/log/install_domain.log
touch $INSTALL_DEPEND_LOG
chmod 755 $INSTALL_DEPEND_LOG

echo "Installing NFS Domain Client, Server IP: $SERVER_IP, Domain: $DOMAIN_NAME"

if [ $(whoami) != "root" ];then
	echo "Please run the domain client installer as root!"
	exit 1;
fi

is64bit=$(getconf LONG_BIT)
if [ "${is64bit}" != '64' ];then
	Red_Error "32-bit systems are not supported; please use 64-bit OS!";
fi

OSType=$(cat /etc/os-release | grep "^ID=" | awk -F '=' '{print $2}')
if [ "${OSType}" == 'nfsdesktop' ];then
	echo -e "Current OS: NFS Desktop"
elif [ "${OSType}" == 'kylin' ];then
	echo -e "Current OS: Kylin Desktop"        
elif [ "${OSType}" == 'uos' ];then
	echo -e "Current OS: UOS Desktop"
else
	echo -e "${OSType} is not supported for NFS Domain Client"
    exit 1
fi

if [ "$SERVER_IP" == "" ];then
	echo "Missing server address parameter!"
	exit 1
fi


OSVersionId=$(cat /etc/os-release | grep "^VERSION_ID=" | awk -F '=' '{print $2}')
KylinPrettyName=$(cat /etc/os-release | grep "^PRETTY_NAME=" | awk -F '=' '{print $2}')

GetSysInfo(){
	if [ -s "/etc/redhat-release" ];then
		SYS_VERSION=$(cat /etc/redhat-release)
	elif [ -s "/etc/issue" ]; then
		SYS_VERSION=$(cat /etc/issue)
	fi
	SYS_INFO=$(uname -a)
	SYS_BIT=$(getconf LONG_BIT)
	MEM_TOTAL=$(free -m|grep Mem|awk '{print $2}')
	CPU_INFO=$(getconf _NPROCESSORS_ONLN)

	echo -e ${SYS_VERSION}
	echo -e Bit:${SYS_BIT} Mem:${MEM_TOTAL}M Core:${CPU_INFO}
	echo -e ${SYS_INFO}
	echo -e "Please send a screenshot of this error to technical support!"
}

Red_Error(){
	echo '=================================================';
	printf '\033[1;31;40m%b\033[0m\n' "$@";
	GetSysInfo
	exit 1;
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
	mv /etc/apt/sources.list /etc/apt/sources.list.bak

	if [ "${OSType}" == 'nfsdesktop' ];then
		if [ "${OSVersionId}" = '"5.0"' ];then
			#mv $sources_dir /etc/apt/sources.list.d.bak
			#mkdir  $sources_dir
			echo "deb [trusted=yes] http://${SERVER_IP}:9778/tiger tiger main" > $server_source
		elif [ "${OSVersionId}" = '4.0.0' ];then
			if [ -f "$server_source" ]; then
				rm "$server_source"
			fi

			for file in $sources_dir/*; do
				if [ "$file" != "/etc/apt/sources.list.d/cdos-appstore.list" ]; then
					mv "$file" "${file}.bak"
				else
					# add cdos-appstore.list #
					sed -i 's/^/#/' /etc/apt/sources.list.d/cdos-appstore.list
				fi
			done
			echo "deb [trusted=yes] http://${SERVER_IP}:9778/nfs4 nfs4 main" > $server_source
		else
			echo ${OSVersionId}
		fi		        
	elif [ "${OSType}" == 'kylin' ]; then
		if [ "${KylinPrettyName}" = '"Kylin V10 SP1"' ];then
			mv $sources_dir /etc/apt/sources.list.d.bak
			mkdir  $sources_dir
			echo "deb [trusted=yes] http://${SERVER_IP}:9778/kylin kylin main" > $server_source
		elif [ "${KylinPrettyName}" = '"Kylin V10"' ];then
			mv $sources_dir /etc/apt/sources.list.d.bak
			mkdir  $sources_dir
			echo "deb [trusted=yes] http://${SERVER_IP}:9778/kylin-v10 kylin-v10 main" > $server_source
		else
			echo ${KylinPrettyName}
		fi			
	elif [ "${OSType}" == 'uos' ]; then
		mv /etc/apt/sources.list.d /etc/apt/sources.list.d.bak
		mkdir  /etc/apt/sources.list.d
		echo "deb [trusted=yes] http://${SERVER_IP}:9778/uos uos main" > $server_source
	fi
	
	if [ -f $CODE_TRUSTED_PART ]; then
		rm -f $CODE_TRUSTED_PART
	fi
	wget -qO - http://${SERVER_IP}:9778/nfs-domain.gpg.key | apt-key add -
}

Install_Check(){
	if [ "${INSTALL_FORCE}" ];then
		return
	fi
	echo -e "----------------------------------------------------"
	echo -e "Existing nfsdomain environment detected; installing may affect existing domain data"
	echo -e "nfsdomain service is alreday installed,Can't install"
	echo -e "----------------------------------------------------"
	echo -e "Risk acknowledged / Enter yes to force installation"
	read -p "Enter yes to force install: " yes;
	if [ "$yes" != "yes" ];then
		echo -e "------------"
		echo "Installation cancelled"
		exit;
	fi
	INSTALL_FORCE="true"
}

Set_HOSTS(){
	hostcheck=$(cat /etc/hosts|grep -c "^${SERVER_IP} ${DOMAIN_NAME}")
	if [ "${hostcheck}" == '1' ];then
		return
	fi

	# 重新写入IP和域名信息
	sed -i "/^[^#].*${DOMAIN_NAME}/ s/^/#/" /etc/hosts
	echo "${SERVER_IP} ${DOMAIN_NAME}" >> /etc/hosts
}

System_Check(){
	SALT_CHECK=$(ps -ef |grep salt-minion|grep -v grep|wc -l)
	if [ "${SALT_CHECK}" != "0" ] ;then
		Install_Check
	fi
}

Install_Domain_Pack(){
	apt-get install ntpdate ldb-tools
	ntpdate -s ${SERVER_IP}

	# 确保pam配置中pam_sss.so能重装
	# apt-get install --reinstall sssd-tools
	
	apt-get install -y chrony
	var=`cat /etc/chrony/chrony.conf | grep "pool" | wc -l`
	if [ $var -ne 0 ]; then  
		sed -i "/^pool / s/^\(.*\)$/#\1/g" /etc/chrony/chrony.conf
	fi

	vars=`cat /etc/chrony/chrony.conf | grep $DOMAIN_NAME | wc -l`
	if [ $vars -eq 0 ]; then
		sed -i "/^#pool /a\server $DOMAIN_NAME iburst" /etc/chrony/chrony.conf
	fi

	systemctl restart chronyd
	systemctl restart chrony
	systemctl enable chrony

	DEBIAN_FRONTEND=noninteractive apt-get install --reinstall -yq freeipa-client 
}

Remove_Domain_Pack() {
	num=$(dpkg -l | grep -c nfs-domain-client)
	if [ $num -gt 0 ]; then
		dpkg -P nfs-domain-client 
		dpkg -P freeipa-client-dbgsym
		dpkg -P freeipa-client
		dpkg -P sssd
	fi
}

install_pkg() {
	name="$1"
	echo "pkg_name=$1"

	if dpkg -l | grep -q "^ii.*${name}" ; then
		current_version=$(dpkg-query -W -f='${Version}' ${name})
		candidate_version=$(apt-cache policy ${name} | egrep "候选|Candidate" | awk -F ' ' '{print $2}')
		echo "current_version: ${current_version}，candidate_version: ${candidate_version}"
		if dpkg --compare-versions ${current_version} lt ${candidate_version}; then
			echo "upgrade  ${name}"
			apt-get install  -y -f --only-upgrade ${name}

		fi
	else
		echo "install ${name}"
		apt-get -y -f install ${name}
	fi
}

Install_Deb_Pack(){
	# kill apt commands process
	pids=$(ps aux | grep apt-get | grep -v grep | awk '{print $2}')
	if [ -n "$pids" ]; then
		kill -9 $pids
	fi

	apt-get update -y
	if [ $? -ne 0 ]; then
		echo "error: failed to update!"
	fi
	
	if [ $DOMAIN_TYPE -eq 1 ]; then
		# install freeipa
		Install_Domain_Pack
	else
		# remove freeipa
		#Remove_Domain_Pack
		echo "unnecessary to remove nfs-domain-client"
	fi

	if [ "${OSType}" == 'nfsdesktop' ];then
		install_pkg cdos-login
		install_pkg nfs-security
		install_pkg novnc 
		install_pkg x11vnc

		if [ "${OSVersionId}" == '"5.0"' ];then
			# 保证版本不低于重构后的版本1.0-m13+1nfs5
			install_pkg nfs-device-disable
		fi
	fi

	install_pkg nfs-domain-client 
     
	# 更新config.ini配置ip_port字段
	num=`grep -c "^add_domain_success=true" $domain_config`
	if [ $num -eq 0 ]; then
		local_ip=`ip addr | grep 'state UP' -A2 | awk '/inet/ {print $2}' | cut -f1 -d'/' | head -n 1`

		# 更新ip_port
		sed -i "s/^ip_port=.*/ip_port=${SERVER_IP}:80/g" $domain_config
		# 更新domain_name
		sed -i "s/^domain_name=.*/domain_name=${DOMAIN_NAME}/g" $domain_config
		# update domain type
		sed -i "s/^domain_type=.*/domain_type=${DOMAIN_TYPE}/g" $domain_config
		# 更新hostname
		sed -i "s/^hostname=.*/hostname=${local_ip}.os-domain.cpu-os.com/g" $domain_config
	fi
	
	# restore source
	if [ "${OSType}" == 'kylin' ] || [ "${OSType}" == 'uos' ]; then
		rm -r $sources_dir
		mv /etc/apt/sources.list.d.bak $sources_dir
	else
		if [ "${OSVersionId}" = '"5.0"' ];then
			echo "do nothing"
			#rm -r $sources_dir
			#mv /etc/apt/sources.list.d.bak $sources_dir
		else
			for file in $sources_dir/*;do
				if [ "$file" != "$server_source" ] && [ "$file" != "$dir/cdos-appstore.list" ]; then
					#mv "$file" "${file%.bak}"
					echo "source list: $file"
				fi
			done
			sed -i 's/^#//' /etc/apt/sources.list.d/cdos-appstore.list
		fi
	fi

    	mv /etc/apt/sources.list.bak /etc/apt/sources.list
}

System_Set(){
	if [ "${OSType}" == 'kylin' ];then	
		setstatus -f exectl off
	fi
}

Install_Main(){
	echo "================install depend============" >> $INSTALL_DEPEND_LOG
	echo $(date "+%Y-%m-%d %H:%M:%S") "start install" >> $INSTALL_DEPEND_LOG
	startTime=`date +%s`

	num=$(dpkg --audit | wc -l)
	if [ $num -gt 0 ]; then
		echo $(dpkg --audit) >> $INSTALL_DEPEND_LOG
		echo "Y" | apt --fix-broken install
	fi

	# 处理川行glx问题
	num=$(dpkg -l | grep -c glx-alternative-zx)
	if [ $num -gt 0 ]; then
		glx_zx_version=`dpkg -s glx-alternative-zx | grep Version | cut -d' ' -f2`
		glx_update_version=`dpkg -s update-glx | grep Version | cut -d' ' -f2`
		if [ "$glx_zx_version" != "$glx_update_version" ]; then
			apt-get remove --yes glx-alternative-zx
		fi
	fi

	# 清理冗余包
	apt autoremove -y

	# System_Check
	System_Set
	Set_HOSTS

	Set_RepRepro

	Install_Deb_Pack

	client_version=$(dpkg-query -W -f='${Version}' nfs-domain-client)
	echo "nfs-domain-client version: ${client_version}" >> $INSTALL_DEPEND_LOG

	if dpkg --compare-versions ${client_version} lt "1.0.1-m50"; then
		echo "failed to update, install package again!"
		Install_Deb_Pack
	fi
}

echo "
+----------------------------------------------------------------------
| NFS Domain Client Installer Script
+----------------------------------------------------------------------
| Copyright © 2006-2022 NFS-CHINA(http://www.nfschina.com) All rights reserved.
+----------------------------------------------------------------------
| useage: sudo bash install.sh serverIP.
+----------------------------------------------------------------------
"

Install_Main
echo -e "=================================================================="
echo -e "\033[32mInstallation succeeded!\033[0m"
echo -e "=================================================================="

endTime=`date +%s`
((outTime=$endTime-$startTime))
echo -e "Time consumed: $outTime seconds!"

exit 0

