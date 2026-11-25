#!/bin/bash

domain_name=$1
domain_controller_ip=$2
user_name=$3
user_passwd=$4
domain_log=$5
touch $domain_log
echo $(date "+%Y-%m-%d %H:%M:%s") "=============================enter==============================." >> $domain_log

if [[ $EUID -ne 0 ]]; then
   echo "This script must be run as root."
   exit 1
fi

if [[ -n `realm list` ]]; then
   echo "has already added to the windows domaion" >> $domain_log
   exit 1
fi

# Install required packages
echo $(date "+%Y-%m-%d %H:%M:%s") "Installing required packages..." >> $domain_log
DEBIAN_FRONTEND=noninteractive apt-get update -y >>$domain_log 2>&1
DEBIAN_FRONTEND=noninteractive apt-get install -y sssd-ad sssd-tools realmd adcli >> $domain_log 2>&1
echo $(date "+%Y-%m-%d %H:%M:%s") "Installing end..." >> $domain_log

# read -p "Enter the AD domain name: " domain_name
# read -p "Enter the IP address of the domain controller: " domain_controller_ip

content="nameserver ${domain_controller_ip}"

if grep -q "$domain_controller_ip" /etc/resolv.conf; then
   echo "Exist IP in /etc/resolv.conf" >> $domain_log
else 
   var=`cat /etc/resolv.conf | grep -n "nameserver" | awk -F: '{print $1}'`
    if [ -z "$var" ]; then
     sed -i  "1i${content}" /etc/resolv.conf
    else
      sed -i "${var}i ${content}" /etc/resolv.conf
    fi
      echo "Add IP in /etc/resolv.conf" >> $domain_log
fi


echo "Discovering the AD domain..." " domain_name:" $domain_name " domain_controller_ip:" $domain_controller_ip "user_name:" "$user_name" >> $domain_log
realm -v discover "$domain_name" >>$domain_log 2>&1
echo $user_passwd | realm --unattended -v join -U "$user_name" "$domain_name" >>$domain_log 2>&1

if [[ -z `realm list` ]]; then
   echo $(date "+%Y-%m-%d %H:%M:%s") "add windows domain failed" >> $domain_log
   exit 1
fi

echo $(date "+%Y-%m-%d %H:%M:%s") "--------------------Start-------------------." >> $domain_log

echo $(date "+%Y-%m-%d %H:%M:%s") "--------Compare sssd.conf start--------." >> $domain_log
if grep -q "^ad_gpo_access_control" /etc/sssd/sssd.conf; then
    sed -i '/^ad_gpo_access_control/d'  /etc/sssd/sssd.conf
fi
sed -i '$a\ad_gpo_access_control = permissive'    /etc/sssd/sssd.conf
echo $(date "+%Y-%m-%d %H:%M:%s") "--------Compare sssd.conf end--------." >> $domain_log

#计算当密码输入成功时需要跳转的行数
get_pam_unix_success_num() {
	file=$1
	pam_unix_line=`grep "^auth" $file | grep -n  "pam_unix.so" |awk -F: '{print $1}'`
   success_line=`grep "^auth"  $file | grep -n  "pam_permit.so" |awk -F: '{print $1}'`
   line=`grep 'pam_nfs_faillock.so' $file | grep 'authsucc' | grep "^auth" | wc -l`
   if [[ $line > 0 ]]; then
	   success_line=`grep "^auth"  $file | grep -n  "pam_nfs_faillock.so" | grep authsucc |awk -F: '{print $1}'`
      echo $(date "+%Y-%m-%d %H:%M:%s") "use pam_nfs_faillock.so in" $file  >> $domain_log
   fi

	success_num=`expr $success_line - $pam_unix_line`
   echo $(date "+%Y-%m-%d %H:%M:%s") "success_num value" $success_num  >> $domain_log
	echo "$success_num"
}

#修改/etc/pam.d/lightdm文件
function modify_lightdm() {
echo $(date "+%Y-%m-%d %H:%M:%s") "--------Compare lightdm start--------." >> $domain_log
OSVersionId=`cat /etc/os-release | grep "^VERSION_ID=" | awk -F'=' '{print  $2}'`
if [ "$OSVersionId" == '"5.0"' ];then
   var=`cat /etc/pam.d/lightdm | grep -n "pam_sss.so" |  wc -l`
   if [ $var -ne 0 ]; then
      sed -i '/pam_sss.so/d' /etc/pam.d/lightdm
      echo $(date "+%Y-m-%d %H:%M:%s") "lightdm remove pam_sss.so first" >> $domain_log
   fi
   var=`cat /etc/pam.d/lightdm | grep -n "pam_unix.so" | awk -F: '{print $1}'`
   if [ -z "$var" ]; then
      echo $(date "+%Y-%m-%d %H:%M:%s") "lightdm missing pam_unix.so configure." >> $domain_log 
   else
      line=`expr $var + 1`
      # 获取未修改前pam_unix.so行对应的success值
      # success_num=`cat /etc/pam.d/lightdm | grep "pam_unix.so" | awk -F "success=" '{print $2}' | awk '{print $1}'`
      success_num=`get_pam_unix_success_num /etc/pam.d/lightdm`
      sed -i ''"${line}"'i\auth\ [success='$[success_num - 1]'\ default=ignore]\ pam_sss.so use_first_pass' /etc/pam.d/lightdm
      echo $(date "+%Y-%m-%d %H:%M:%s") "lightdm add pam_sss.so" >> $domain_log
      var=`cat /etc/pam.d/lightdm | grep -n "pam_unix.so" | awk -F: '{print $1}'`
      if [ ! -z "$var" ];then
         sed -i ''"$[var]"'s/success=[0-9]*/success='${success_num}'/g' /etc/pam.d/lightdm
         echo $(date "+%Y-%m-%d %H:%M:%s") " lightdm pam_unix.so change  success to ${success_num} ." >> $domain_log
      fi
      var=`cat /etc/pam.d/lightdm | grep -n "pam_nfs_auth.so" | awk -F: '{print $1}'`
      # 获取未修改前pam_nfs_auth.so行对应的success值
      # success_num=`cat /etc/pam.d/lightdm | grep "pam_nfs_auth.so" | awk -F "success=" '{print $2}' | awk '{print $1}'`
      if [ ! -z "$var" ]; then
         sed -i ''"$[var]"'s/success=[0-9]*/success='$[success_num + 1]'/g' /etc/pam.d/lightdm
         echo $(date "+%Y-%m-%d %H:%M:%s") " lightdm pam_nfs_auth.so change  success to $[success_num + 1] ." >> $domain_log
      fi
   fi
fi
echo $(date "+%Y-%m-%d %H:%M:%s") "--------Compare lightdm end--------." >> $domain_log
}


#修改/etc/pam.d/common-auth
# 会自动生成pam_sss
function modify_auth() {
echo $(date "+%Y-%m-%d %H:%M:%s") "--------Compare common-auth start--------." >> $domain_log
var=`cat /etc/pam.d/common-auth | grep -n "pam_sss.so" |  wc -l`
if [ $var -ne 0 ]; then
   sed -i '/pam_sss.so/d' /etc/pam.d/common-auth
   echo $(date "+%Y-m-%d %H:%M:%s") "common-auth remove pam_sss.so first" >> $domain_log
fi
var=`cat /etc/pam.d/common-auth | grep -e "pam_faillock.so" -e "pam_nfs_faillock.so" | grep preauth | grep "^auth" | wc -l`
if [ $var -ne 0 ]; then
   line=`cat /etc/pam.d/common-auth | grep -n -e "pam_faillock.so" -e "pam_nfs_faillock.so" | grep preauth | awk -F: '{print $1}'`
   sed -i ''"${line}"'s/^/#&/g' /etc/pam.d/common-auth
fi
var=`cat /etc/pam.d/common-auth | grep -e "pam_faillock.so" -e "pam_nfs_faillock.so" | grep authfail | grep "^auth" | wc -l`
if [ $var -ne 0 ]; then

   var=`cat /etc/pam.d/common-auth | grep -n -e "pam_faillock.so" -e "pam_nfs_faillock.so" | grep  authfail| awk -F: '{print $1}'`
   sed -i ''"${var}"'s/^/#&/g' /etc/pam.d/common-auth
fi
success_num=`get_pam_unix_success_num /etc/pam.d/common-auth`
sed -i '/pam_unix\.so nullok/s/success=[0-9]*/success='${success_num}'/g' /etc/pam.d/common-auth
echo $(date "+%Y-%m-%d %H:%M:%s") " common-auth pam_unix.so change  success to ${success_num} ." >> $domain_log
var=`cat /etc/pam.d/common-auth | grep "pam_sss.so" | wc -l`
if [ $var -eq 0 ]; then
var=`cat /etc/pam.d/common-auth | grep -n "pam_unix.so" | awk -F: '{print $1}'`
   if [ -z "$var" ]; then
      echo $(date "+%Y-%m-%d %H:%M:%s") "common-auth missing pam_unix.so configure." >> $domain_log 
   else
      line=`expr $var + 1`
      sed -i ''"${line}"'i\auth\ [success='$[success_num - 1]'\ default=ignore]\ pam_sss.so use_first_pass' /etc/pam.d/common-auth
       echo $(date "+%Y-m-%d %H:%M:%s") "common-auth add pam_sss.so" >> $domain_log
   fi
fi

echo $(date "+%Y-%m-%d %H:%M:%s") "--------Compare common-auth end--------." >> $domain_log
}


#修改/etc/pam.d/cdos-screensaver文件
function modify_screensaver() {
echo $(date "+%Y-%m-%d %H:%M:%s") "--------Compare cdos-screensaver start--------." >> $domain_log
var=`cat /etc/pam.d/cdos-screensaver | grep -n "pam_sss.so" |  wc -l`
if [ $var -ne 0 ]; then
   sed -i '/pam_sss.so/d' /etc/pam.d/cdos-screensaver
   echo $(date "+%Y-m-%d %H:%M:%s") "cdos-screensaver remove pam_sss.so first" >> $domain_log
fi
var=`cat /etc/pam.d/cdos-screensaver | grep -n "pam_unix.so nullok_secure" | awk -F: '{print $1}'`
if [ -z "$var" ];then
   echo $(date "+%Y-m-%d %H:%M:%s") "cdos-screensaver missing pam_unix.so configure." >> $domain_log
else
   # success_num=`cat /etc/pam.d/cdos-screensaver | grep "pam_unix.so" | awk -F "success=" '{print $2}' | awk '{print $1}'`
   success_num=`get_pam_unix_success_num /etc/pam.d/cdos-screensaver`
   sed -i ''"${var}"'s/success=[0-9]*/success='${success_num}'/g' /etc/pam.d/cdos-screensaver
   echo $(date "+%Y-%m-%d %H:%M:%s") " cdos-screensaver pam_unix.so change  success to ${success_num} ." >> $domain_log
   line=`cat /etc/pam.d/cdos-screensaver | grep "pam_sss.so" | wc -l`
   if [ $line -eq 0 ]; then
      line=`expr $var + 1`
      #sed -i ''"${line}"'i\auth\ [success=1\ default=ignore]\     pam_sss.so use_first_pass' /etc/pam.d/cdos-screensaver
      sed -i ''"${line}"'i\auth\ [success='$[success_num - 1]'\ default=ignore]\     pam_sss.so use_first_pass' /etc/pam.d/cdos-screensaver
      echo $(date "+%Y-%m-%d %H:%M:%s") "cdos-screensaver add pam_sss.so" >> $domain_log
      #sed -i ''"${line}"'s/success=1/success=3/g' /etc/pam.d/cdos-screensaver
      #sed -i ''"${line}"'i\auth\ [success=2\ default=ignore]\     pam_sss.so nullok_secure' /etc/pam.d/cdos-screensaver
      #sed -i ''"$var"'d' etc/pam.d/cdos-screensaver
   fi
fi

#TODO
# 生物认证pam文件cdos-screensaver中配置默认也是正确，无需修改

#var=`cat /etc/pam.d/cdos-screensaver | grep -n "pam_fingerprint.so" | awk -F: '{print $1}'`

#if [ -z "$var" ];then
#echo $(date "+%Y-m-%d %H:%M:%s") "cdos-screensaver missing pam fingerprint.so configure."
#else
#  line=`expr $var + 1`
#  #sed -i ''"${line}"'i\auth\ [success=2\ default=ignore]\      pam_sss.so use_first_pass' /etc/pam.d/cdos-screensaver
#  #sed -i ''"${line}"'i\auth\ [success=2\ default=ignore]\      pam_fingerprint.so wait_readpw'  /etc/pam.d/cdos-screensaver
#  #sed -i ''"$var"'d' /etc/pam.d/cdos-screensaver
#fi
echo $(date "+%Y-%m-%d %H:%M:%s") "--------Compare cdos-screensaver end--------." >> $domain_log
}


#修改/etc/pam.d/common-session
function modify_session() {
# 会自动生成pam_sss
echo $(date "+%Y-%m-%d %H:%M:%s") "--------Compare common-session start--------." >> $domain_log
var=`cat /etc/pam.d/common-session | grep "pam_sss.so" | wc -l`
if [ $var -eq 0 ]; then
   sed -i '$a\session\  optional\ pam_sss.so' /etc/pam.d/common-session
   echo $(date "+%Y-%m-%d %H:%M:%s") "common-session add pam_sss.so" >> $domain_log 
fi

var=`cat /etc/pam.d/common-session | grep "pam_mkhomedir.so" | wc -l`
if [ $var -eq 0 ]; then 
   sed -i '$a\session\  optional\ pam_mkhomedir.so\  skel=/etc/skel/\ umask=077'    /etc/pam.d/common-session
   echo $(date "+%Y-m-%d %H:%M:%s") "common-session add pam_mkhomedir.so" >> $domain_log
fi
   echo $(date "+%Y-%m-%d %H:%M:%s") "--------Compare common-session end--------." >> $domain_log
}


modify_lightdm
modify_auth
modify_screensaver
modify_session
echo $(date "+%Y-%m-%d %H:%M:%s") "--------------------End--------------------." >> $domain_log
exit 0




