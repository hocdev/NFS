#!/bin/bash

logfile=/var/log/nfs-domain-client/update_pam.log

if [ ! -f $logfile ]; then 
    touch $logfile
    chmod 755 $logfile
fi
echo $(date "+%Y-%m-%d %H:%M:%S") "ready to add pam settings" >> $logfile

var=`grep -c "pam_sss.so use_first_pass" /etc/pam.d/common-auth`
if [ $var -eq 0 ]; then
    echo  $(date "+%Y-%m-%d %H:%M:%S") "common-auth missing pam_sss.so configure." >> $logfile
fi

var=`grep -c "pam_localuser.so" /etc/pam.d/common-account`
if [ $var -eq 0 ]; then
    echo  $(date "+%Y-%m-%d %H:%M:%S") "common-account missing pam_localuser.so configure." >> $logfile
    echo "account sufficient        pam_localuser.so" >> /etc/pam.d/common-account
fi

var=`grep -c "pam_sss.so" /etc/pam.d/common-account`
if [ $var -eq 0 ]; then
    echo  $(date "+%Y-%m-%d %H:%M:%S") "common-account missing pam_sss.so configure." >> $logfile
    echo "account [default=bad success=ok user_unknown=ignore]        pam_sss.so" >> /etc/pam.d/common-account
fi

var=`grep -c "pam_sss.so" /etc/pam.d/common-session`
if [ $var -eq 0 ]; then
    sed -i '$a\session\ optional\   pam_sss.so' /etc/pam.d/common-session
    echo  $(date "+%Y-%m-%d %H:%M:%S") "common-session add pam_sss.so configure. result: $?" >> $logfile
fi

var=`grep -c "pam_mkhomedir.so" /etc/pam.d/common-session`
if [ $var -eq 0 ]; then
    sed -i '$a\session\ optional\   pam_mkhomedir.so\ skel=/etc/skel/\ umask=077' /etc/pam.d/common-session
    echo  $(date "+%Y-%m-%d %H:%M:%S") "common-session add pam_mkhomedir.so configure. result: $?" >> $logfile
    cat /etc/pam.d/common-session >> $logfile
fi

PAM_LOGIN_FILE="/etc/pam.d/login"
# 检查文件是否存在
if [ -f "$PAM_LOGIN_FILE" ]; then
    # 检查是否包含未被注释的 pam_lastlog.so
    if grep -q '^[^#]*pam_lastlog.so' "$PAM_LOGIN_FILE"; then
        # 在 pam_lastlog.so 行前面加上 # 进行注释
        sed -i '/^[^#]*pam_lastlog.so/s/^/#/' "$PAM_LOGIN_FILE"
        echo $(date "+%Y-%m-%d %H:%M:%S") "pam_lastlog.so has been commented out in $PAM_LOGIN_FILE."  >> $logfile
    else
        echo $(date "+%Y-%m-%d %H:%M:%S") "pam_lastlog.so is not present in $PAM_LOGIN_FILE."  >> $logfile
    fi
else
    echo $(date "+%Y-%m-%d %H:%M:%S") "$PAM_LOGIN_FILE does not exist."  >> $logfile
fi

# 用于计算success值
get_pam_unix_success_num() {
	file=$1
	pam_unix_line=`grep "^auth" $file | grep -n  "pam_unix.so" |awk -F: '{print $1}'`
	pam_deny_line=`grep "^auth"  $file | grep -n  "pam_deny.so" |awk -F: '{print $1}'`

	success_num=`expr $pam_deny_line - $pam_unix_line`
	echo "$success_num"
}

OSVersionId=$(cat /etc/os-release | grep "^VERSION_ID=" | awk -F '=' '{print $2}')

if [ "$OSVersionId" == '4.0.0' ];then
    var=`grep -c "pam_unix.so" /etc/pam.d/lightdm`
    if [ $var -ne 0 ]; then
        sed -i '/pam_unix.so/d' /etc/pam.d/lightdm
    fi

    var=`grep -c  "pam_sss.so" /etc/pam.d/lightdm`
    if [ $var -ne 0 ]; then
        sed -i '/pam_sss.so/d' /etc/pam.d/lightdm
    fi

    var=`grep -c  "pam_fingerprint.so" /etc/pam.d/lightdm`
    if [ $var -eq 0 ]; then
        echo  $(date "+%Y-%m-%d %H:%M:%S") "lightdm missing pam_fingerprint.so configure." >> $logfile
    else
        line=`expr $var + 1`
        sed -i ''"${line}"'i\auth\ [success=1\ default=ignore]\	pam_sss.so' /etc/pam.d/lightdm
        sed -i ''"${line}"'i\auth\ [success=2\ default=ignore]\	pam_fingerprint.so wait_readpw' /etc/pam.d/lightdm
    
        sed -i ''"$var"'d' /etc/pam.d/lightdm
    fi
elif [ "$OSVersionId" == '"5.0"' ];then
    is_shyh=`dpkg -l | grep -c pam-nfs-shyh-httpauth`
    echo "check pam-nfs-shyh-httpauth existed: $is_shyh" >> $logfile

    if [ $is_shyh -eq 1 ]; then
	# shanghai bank仅当/etc/pam.d/lightdm中没有pam_sss.so时才添加
	echo "has pam_sss.so in lightdm pam" >> $logfile
    else
        sed -i '/pam_sss.so/d' /etc/pam.d/lightdm
	echo "add pam_sss.so to lightdm pam" >> $logfile
	var=`cat /etc/pam.d/lightdm | grep -n  "pam_unix.so" | awk -F: '{print $1}'`
	if [ -z "$var" ]; then
	    echo  $(date "+%Y-%m-%d %H:%M:%S") "lightdm missing pam_unix.so configure." >> $logfile
	else
	    line=`expr $var + 1`
	    sed -i ''"${line}"'i\auth\ [success=2\ default=ignore]\	pam_sss.so use_first_pass' /etc/pam.d/lightdm

	    var=`cat /etc/pam.d/lightdm | grep -n  "pam_unix.so" |awk -F: '{print $1}'`
	    if [ ! -z "$var" ]; then
		sed  -i ''"${var}"'s/success=2/success=3/g' /etc/pam.d/lightdm
	    fi

	    var=`cat /etc/pam.d/lightdm | grep -n  "pam_nfs_auth.so" |awk -F: '{print $1}'`
	    if [ ! -z "$var" ]; then
		sed  -i ''"${var}"'s/success=3/success=4/g' /etc/pam.d/lightdm
	    fi
	fi
    fi

    if [ $is_shyh -eq 1 ]; then
	# shanghai bank
	mv /etc/pam.d/common-auth  /etc/pam.d/common-auth_bak
cat << EOF > /etc/pam.d/common-auth
# customized for shanghaibank
# here are the per-package modules (the "Primary" block)
auth	requisite	pam_nfs_faillock.so preauth
auth	[success=3 default=ignore]	pam_unix.so nullok try_first_pass
auth	[success=2 default=ignore]	pam_sss.so use_first_pass
auth	[default=die]	pam_nfs_faillock.so authfail
# here's the fallback if no module succeeds
auth	requisite			pam_deny.so
# prime the stack with a positive return value if there isn't one already;
# this avoids us returning an error just because nothing sets a success code
# since the modules above will each just jump around
auth	optional	pam_nfs_faillock.so authsucc
auth	required			pam_permit.so
# and here are more per-package modules (the "Additional" block)
# end of pam-auth-update config
EOF
    else
	    var=`grep -e "pam_faillock.so" -e "pam_nfs_faillock.so" /etc/pam.d/common-auth  | grep preauth | grep "^auth"| wc -l`
	    if [ $var -ne 0 ]; then
		line=`cat /etc/pam.d/common-auth | grep -n -e "pam_faillock.so" -e "pam_nfs_faillock.so" | grep preauth | awk -F: '{print $1}'`
		sed  -i ''"${line}"'s/^/#&/g' /etc/pam.d/common-auth
	    fi
	    var=`cat /etc/pam.d/common-auth | grep -e "pam_faillock.so" -e "pam_nfs_faillock.so" | grep authfail | grep "^auth"| wc -l`
	    if [ $var -ne 0 ]; then
		line=`cat /etc/pam.d/common-auth | grep -n -e "pam_faillock.so" -e "pam_nfs_faillock.so" | grep authfail | awk -F: '{print $1}'`
		 sed  -i ''"${line}"'s/^/#&/g' /etc/pam.d/common-auth
	    fi

	    var=`cat /etc/pam.d/common-auth | grep -n  "pam_sss.so" | awk -F: '{print $1}'`
	    if [ ! -z "$var" ]; then
	        sed -i ''"${var}"'d' /etc/pam.d/common-auth
	    fi
	
	    # 更新pam_sss.so和pam_unix.so的success值
	    var=`cat /etc/pam.d/common-auth | grep -n  "pam_unix.so nullok" | awk -F: '{print $1}'`
	    line=`expr $var + 1`
	    sss_success_num=`get_pam_unix_success_num /etc/pam.d/common-auth`
	    unix_success_num=`expr $sss_success_num + 1`

	    # 修正pam_unix.so的success值
	    sed -i "${line}i\auth    [success=$unix_success_num default=ignore]      pam_unix.so nullok_secure" /etc/pam.d/common-auth
    	    sed -i ''"$var"'d' /etc/pam.d/common-auth

	    # 插入pam_sss.so
	    sed -i "${line}i\auth    [success=$sss_success_num default=ignore]      pam_sss.so use_first_pass" /etc/pam.d/common-auth
	    # 修正pam_nfs_auth.so
	    var=`cat /etc/pam.d/common-auth | grep -n  "pam_nfs_auth.so" |awk -F: '{print $1}'`
	    if [ ! -z "$var" ]; then
		sed  -i ''"${var}"'s/success=4/success=3/g' /etc/pam.d/common-auth
	    fi
    fi
else
    echo "system version:"  $OSVersionId
fi



echo  $(date "+%Y-%m-%d %H:%M:%S") "lightdm  configure." >> $logfile


var=`cat /etc/pam.d/cdos-screensaver | grep -n  "pam_sss.so" | awk -F: '{print $1}'`
if [ ! -z "$var" ]; then
    sed -i ''"${var}"'d' /etc/pam.d/cdos-screensaver
fi

var=`cat /etc/pam.d/cdos-screensaver | grep -n  "pam_unix.so nullok_secure" | awk -F: '{print $1}'`

if [ -z "$var" ]; then
    echo  $(date "+%Y-%m-%d %H:%M:%S") "cdos-screensaver missing pam_unix.so configure." >> $logfile
else
    line=`expr $var + 1`
    sss_success_num=`get_pam_unix_success_num /etc/pam.d/cdos-screensaver`
    unix_success_num=`expr $sss_success_num + 1`
    # 修正pam_unix.so的success值
    sed -i "${line}i\auth    [success=$unix_success_num default=ignore]      pam_unix.so nullok_secure" /etc/pam.d/cdos-screensaver
    sed -i ''"$var"'d' /etc/pam.d/cdos-screensaver

    # 插入pam_sss.so
    sed -i "${line}i\auth    [success=$sss_success_num default=ignore]      pam_sss.so use_first_pass" /etc/pam.d/cdos-screensaver
fi

var=`cat /etc/pam.d/cdos-screensaver | grep -n  "pam_fingerprint.so" | awk -F: '{print $1}'`

if [ -z "$var" ]; then
    echo  $(date "+%Y-%m-%d %H:%M:%S") "cdos-screensaver missing pam_fingerprint.so configure." >> $logfile
else
    line=`expr $var + 1`
    sed -i ''"${line}"'i\auth\ [success=1\ default=ignore]\	pam_sss.so' /etc/pam.d/cdos-screensaver
    sed -i ''"${line}"'i\auth\ [success=2\ default=ignore]\	pam_fingerprint.so wait_readpw' /etc/pam.d/cdos-screensaver
   
    sed -i ''"$var"'d' /etc/pam.d/cdos-screensaver
fi

exit 0
