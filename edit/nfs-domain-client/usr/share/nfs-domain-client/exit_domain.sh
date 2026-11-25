#!/bin/bash
########################################################################################
# 				fallback 终端单独退域脚本                              #
########################################################################################
LOG_DIR=/var/log/nfs-domain-client
LOG_PATH="$LOG_DIR/exit_domain.log"
LOG_INFO="Info"
LOG_ERROR="Error"
CONFIG_PATH=/usr/share/nfs-domain-client/salt/config.ini
DOMAIN_NAME=`grep domain_name $CONFIG_PATH | awk -F= '{print $2}'`
SERVER_IP=`grep ip_port $CONFIG_PATH  | awk -F= '{print $2}' | awk -F: '{print $1}'`
is_shyh=`dpkg -l | grep -c pam-nfs-shyh-httpauth`
nfs_tiger="5.0"
nfs_4="4.0.0"

print_log()
{
	log_level=$1
	info=$2
	now=`date "+%Y-%m-%d %H:%M:%S"`
	echo "$now [$log_level]: $info" >> $LOG_PATH
}

if [ $(id -u) -ne 0 ]; then
	print_log $LOG_INFO "you should run this script with sudo!"
	exit 1
fi

if [ ! -d "$LOG_DIR" ]; then
	mkdir $LOG_DIR
fi

if [ ! -f "$LOG_PATH" ]; then
	touch $LOG_PATH
fi

########################################################################################
# 解锁本地用户
########################################################################################
unlock_local_user() {
	userlist=`awk -F: '$3>=1000 && $3 < 65534 && $4>=1000 && $4 < 65534' /etc/passwd |  awk -F: '{print $1}'`
	failcount=0
	for user in $userlist
	do
	    usermod -U $user
	    if [ $? -eq 0 ]; then
		print_log $LOG_INFO "unlock user $user successful."
	    else
		print_log $LOG_ERROR "unlock user $user fail."
		failcount=`expr $failcount + 1`
	    fi

	done

	if [ "$failcount" -eq 0 ]; then
	    print_log $LOG_INFO "unlock user successful."
	else
	    print_log $LOG_ERROR "failed to unlock user"
	fi
}


########################################################################################
# 恢复pam认证流程
########################################################################################
get_pam_unix_success_num() {
	file=$1
	pam_unix_line=`grep "^auth" $file | grep -n  "pam_unix.so" |awk -F: '{print $1}'`
	pam_deny_line=`grep "^auth"  $file | grep -n  "pam_deny.so" |awk -F: '{print $1}'`

	success_num=`expr $pam_deny_line - $pam_unix_line`
	print_log $LOG_INFO "get_pam_unix_success_num: $success_num"
}

recover_pam() {
	var=`cat /etc/pam.d/common-auth | grep "pam_sss.so use_first_pass" | wc -l`
	if [ $var -ne 0 ] && [ $is_shyh -ne 1 ]; then
	    sed -i '/pam_sss.so use_first_pass/d' /etc/pam.d/common-auth
	fi

	var=`cat /etc/pam.d/common-account | grep "pam_localuser.so" | wc -l`
	if [ $var -ne 0 ]; then  
	    sed -i '/pam_localuser.so/d' /etc/pam.d/common-account
	fi

	var=`cat /etc/pam.d/common-account | grep "pam_sss.so" | wc -l`
	if [ $var -ne 0 ]; then  
	    sed -i '/pam_sss.so/d' /etc/pam.d/common-account
	fi

	var=`cat /etc/pam.d/common-session | grep "pam_sss.so" | wc -l`
	if [ $var -ne 0 ]; then   
	    sed -i '/pam_sss.so/d' /etc/pam.d/common-session
	fi
	var=`cat /etc/pam.d/common-session | grep "pam_mkhomedir.so" | wc -l`
	if [ $var -ne 0 ]; then
	    sed -i '/pam_mkhomedir.so/d' /etc/pam.d/common-session
	fi

	OSVersionId=$(cat /etc/os-release | grep "^VERSION_ID=" | awk -F '=' '{print $2}' | tr -d '"')
	print_log $LOG_INFO "get os version: $OSVersionId"
	print_log $LOG_INFO "get   is_shyh: $is_shyh"
	 
	if [ "$OSVersionId" = "$nfs_4" ]; then
	    var=`cat /etc/pam.d/lightdm | grep -n  "pam_sss.so" | awk -F: '{print $1}'`
	    if [ -z "$var" ]; then
			print_log $ERROR "lightdm missing pam_sss.so configure."
	    else
			sed -i ''"${var}"'d' /etc/pam.d/lightdm
	    fi

	    var=`cat /etc/pam.d/lightdm | grep -n  "pam_fingerprint.so" | awk -F: '{print $1}'`
	    if [ -z "$var" ]; then
			print_log $ERROR "lightdm missing pam_fingerprint.so configure."
	    else
			line=`expr $var + 1`
			sed -i ''"${line}"'i\auth\ [success=1\ default=ignore]\	pam_fingerprint.so wait_readpw' /etc/pam.d/lightdm    
			sed -i ''"${var}"'d' /etc/pam.d/lightdm
	    fi
	elif [ "$OSVersionId" = "$nfs_tiger" ] && [ "$is_shyh" -ne 1 ]; then
	    # shanghai bank 统一认证；加域和退域都不需要修改lightdm common-authpam
	    var=`cat /etc/pam.d/lightdm | grep -n  "pam_unix.so" |awk -F: '{print $1}'`
	    if [ ! -z "$var" ]; then
			sed  -i ''"${var}"'s/success=3/success=2/g' /etc/pam.d/lightdm
	    fi

	    var=`cat /etc/pam.d/lightdm | grep -n  "pam_nfs_auth.so" |awk -F: '{print $1}'`
	    if [ ! -z "$var" ]; then
			sed  -i ''"${var}"'s/success=4/success=3/g' /etc/pam.d/lightdm
	    fi
	    
	    var=`cat /etc/pam.d/lightdm | grep "pam_sss.so" | wc -l`
	    if [ $var -ne 0 ]; then
			sed -i '/pam_sss.so/d' /etc/pam.d/lightdm
	    fi

	    var=`cat /etc/pam.d/common-auth | grep -e "pam_faillock.so" -e "pam_nfs_faillock.so" | grep preauth | grep "^#auth"| wc -l`
	    if [ $var -ne 0 ]; then
			line=`cat /etc/pam.d/common-auth | grep -n -e"pam_faillock.so" -e "pam_nfs_faillock.so" | grep preauth | awk -F: '{print $1}'`
			sed  -i ''"${line}"'s/^#//g' /etc/pam.d/common-auth
	    fi

	    var=`cat /etc/pam.d/common-auth | grep -e "pam_faillock.so" -e "pam_nfs_faillock.so" | grep authfail | grep "^#auth"| wc -l`
	    if [ $var -ne 0 ]; then
			line=`cat /etc/pam.d/common-auth | grep -n -e "pam_faillock.so" -e "pam_nfs_faillock.so" | grep authfail | awk -F: '{print $1}'`
			sed  -i ''"${line}"'s/^#//g' /etc/pam.d/common-auth
	    fi

	    var=`cat /etc/pam.d/common-auth | grep "pam_sss.so" | wc -l`
	    if [ $var -ne 0 ]; then
			sed -i '/pam_sss.so/d' /etc/pam.d/common-auth
	    fi

	else
	    print_log $LOG_INFO "other os version: $OSVersionId"
	fi

	print_log $LOG_INFO "restore lightdm  configure."

	var=`cat /etc/pam.d/cdos-screensaver | grep -n  "pam_sss.so" | awk -F: '{print $1}'`
	if [ -z "$var" ]; then
		print_log $LOG_ERROR "cdos-screensaver missing pam_sss.so configure." 
	else
		sed -i ''"${var}"'d' /etc/pam.d/cdos-screensaver
	fi


	var=`cat /etc/pam.d/cdos-screensaver | grep -n  "pam_unix.so nullok_secure" | awk -F: '{print $1}'`
	if [ -z "$var" ]; then
	    print_log $LOG_ERROR "cdos-screensaver missing pam_unix.so configure."
	else
	    line=`expr $var + 1`
	    success_num=`get_pam_unix_success_num /etc/pam.d/cdos-screensaver`
	    # 根据计算值设置success值
    	    sed -i "${line}i\auth    [success=$success_num default=ignore]      pam_unix.so nullok_secure" /etc/pam.d/cdos-screensaver    
	    sed -i ''"${var}"'d' /etc/pam.d/cdos-screensaver
	fi

	print_log $LOG_INFO "restore cdos-screensaver  configure."

	PAM_LOGIN_FILE="/etc/pam.d/login"

	if [ -f "$PAM_LOGIN_FILE" ]; then
		if grep -q '^#.*pam_lastlog.so' "$PAM_LOGIN_FILE"; then
			# 解除注释 pam_lastlog.so 行
			sed -i '/^#.*pam_lastlog.so/s/^#//' "$PAM_LOGIN_FILE"
			print_log $LOG_INFO "Commented pam_lastlog.so has been uncommented in $PAM_LOGIN_FILE."
		else
			print_log $LOG_ERROR "Commented pam_lastlog.so is not present in $PAM_LOGIN_FILE."
		fi
	else
		print_log $LOG_ERROR "$PAM_LOGIN_FILE does not exist."
	fi

}


########################################################################################
# 标记退域文件
########################################################################################
mark_config() {
	CONFIG_PATH=/usr/share/nfs-domain-client/salt/config.ini
	CURRENT_TIME=`date "+%Y%m%d-%H%M%S"`

	# 记录每次退域时间
	sed -i "/^add_domain_success=true/s/$/ # changed in time:$CURRENT_TIME/" $CONFIG_PATH
	# 增加当前状态
	sed -i "/^add_domain_success=true/a\add_domain_success=false" $CONFIG_PATH
	# 用；注释之前的加域信息
	sed -i "/^add_domain_success=true/s/^/;/" $CONFIG_PATH
}


main() {
	chmod 666 $LOG_PATH
	print_log $LOG_INFO "ready to exit domain!"

	# 恢复ssh
	if [ -f "/etc/ssh/sshd_config_bak" ]; then
		cp /etc/ssh/sshd_config_bak /etc/ssh/sshd_config
		if [ $? -ne 0 ];then
			print_log $LOG_ERROR "failed to recover sshd_config"
		fi
	fi
	
	systemctl restart ssh

	# 解锁本地用户登录
	unlock_local_user

	# freeipa退域
	ipa-client-install --uninstall  --unattended
	if [ $? -ne 0 ];then
		print_log $LOG_ERROR "failed to exec ipa-client-install --uninstall --unattended "
	fi
	# delete ipa default.conf
	if [ -f "/etc/ipa/default.conf" ]; then
		print_log $LOG_INFO "delete ipa default.conf"
		rm /etc/ipa/default.conf
	else
		print_log $LOG_INFO "default.conf is deleted"
	fi
	# update realm cache
	systemctl restart realmd >> $LOG_PATH

	#退域情况，停用NS中sss, 停用sssd服务
	# if [ -f "/etc/sssd/sssd.conf" ]; then
		print_log $LOG_INFO "disable sssd.service"
		systemctl disable sssd.service
		systemctl stop sssd.service

		num=`grep -c "sudoers:.*sss" /etc/nsswitch.conf `
		if [ $num -ne 0 ]; then  
			print_log $LOG_INFO "disable sudoers: sss in nsswitch.conf"
			sed -i "/^sudoers/ s/^\(.*\)$/#\1/g" /etc/nsswitch.conf
		fi
	# fi

	# freeip退域会自动清除
	# 清除sssd配置
	#rm -rf /etc/sssd/*
	#if [ $? -ne 0 ];then
	#	print_log $LOG_ERROR "failed to clear /etc/sssd/*"
	#fi

	# 恢复pam认证流程
	recover_pam


	# 删除salt-minion缓存
	# rm -rf /etc/salt/minion_id  /etc/salt/minion  /var/cache/salt /etc/salt/pki/minion/minion_master.pub
	rm -rf /var/cache/salt

	# 关闭客户端与服务端通信
	#systemctl disable salt-minion
	#systemctl stop salt-minion

	# 清除DNS
	sed -i "/${SERVER_IP}/d" /etc/resolv.conf
	sed -i "/${SERVER_IP}/d" /etc/hosts

	# 恢复chrony
	sed -i "s/^#pool/pool/" /etc/chrony/chrony.conf
	sed -i "/${DOMAIN_NAME}/d" /etc/chrony/chrony.conf

	# 取消自启动
	systemctl disable nfsdomain.service

	# 停止nfs-domain-agent进程
	domain_agent=$(pidof nfs-domain-agent)
	if [ -n "${domain_agent}" ];then
	    #nfsdomain_status=$(systemctl is-active nfsdomain.service)
	    print_log $LOG_INFO "nfsdomain.service stautus: $nfsdomain_status ,nfs-domain-agent pid: ${domain_agent}"
	    #if [ "$nfsdomain_status" = "active" ]; then
	        #systemctl stop nfsdomain.service
	        #systemctl disable nfsdomain.service
	        #echo "stop nfsdomain.service"
	    #else
	        #killall nfs-domain-agent
	        #echo "manual killall  nfs-domain-agent"
	    #fi
	fi

	# call UncacheUser
	# username=$(grep username $CONFIG_PATH | awk -F= '{print $2}')
	#users=${ldbsearch  -H /var/lib/sss/db/cache_os-domain.cpu-os.com.ldb  |grep "dn: name=" | awk -F "dn: name="  '{print $2}' | awk -F "@" '{print $1}}
	# dbus-send --system --dest=org.freedesktop.Accounts --type=method_call \
    #             --print-reply /org/freedesktop/Accounts \
    #             org.freedesktop.Accounts.UncacheUser string:"$username" >> $LOG_PATH

	# 清除本地所有域缓存信息, sss_cache -E并没有清理干净缓存的数据
	# ipa-client-install不会自动清理掉服务端数据库中该终端上的用户绑定关系
	rm /var/lib/sss/db/*

	# recover /etc/hosts
	if [ -f /etc/hosts.fdgkgj ]; then
		mv /etc/hosts.fdgkgj /etc/hosts
	else
		hostname=$(cat /usr/share/nfs-domain-client/salt/config.ini | grep "^hostname=" | awk -F '=' '{print $2}')
		sed -i "/${hostname}/d" /etc/hosts
	fi
	print_log $LOG_INFO "over to run main!"
	# 标记config加域状态
	mark_config

	chattr -i /usr/share/nfs-domain-client/sssd.conf
	print_log $LOG_INFO  "set chattr -i"
	# remove backup sssd.conf
	rm /usr/share/nfs-domain-client/sssd.conf
	exit 0
}

# 加域失败，恢复pam认证流程
if [ -n "$1" ] && [ "$1" = "recover_pam" ]; then
	#未加域情况
	if [ ! -f "/etc/sssd/sssd.conf" ]; then
	 recover_pam
	else
	 print_log $LOG_INFO "it is in domain ,dont need recover pam!"
	fi
	exit 0
fi

main
