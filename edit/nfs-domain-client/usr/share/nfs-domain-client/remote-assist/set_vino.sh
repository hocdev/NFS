#! /bin/bash

# 根据规则设置vino的gsetting 属性

log=/var/log/nfs-domain-client/vino.log
touch $log
chmod 666 $log
echo "gsetting set vino" >> $log
now=`date "+%Y-%m-%d %H:%M:%S"`
echo  $now >>$log
#1允许远程桌面连接控制您的桌面 enabled isAllowdRemoteDesktop
#2每次连接都需要得到您的同意  needPromptEnable
#3要求输入访问密码 needpasswd 
#4访问密码 passwd remotePwd

is_allowd_remote_desktop=$1
prompt_enabled=$2
is_need_pwd=$3
remote_pwd=$4
view_only=false

echo  "is_allowd_remote_desktop: $is_allowd_remote_desktop  prompt_enabled: $prompt_enabled is_need_pwd: $is_need_pwd  is_need_pwd: $remote_pwd"  >> $log

current_user=`who | grep "(:0"  | awk '{print $1}'`
if [ -n "$current_user" ]; then
	echo "found user used :0" >> $log
else
	current_user=`who | grep "(:1" | awk '{print $1}'`
fi

userid=$(id -u ${current_user})

echo "login username:" $current_user ", userid:" $userid >> $log

 
if [ "$is_allowd_remote_desktop" = "1" ]; then
	is_allowd_remote_desktop_val=true
else
	is_allowd_remote_desktop_val=false
fi

if [ "$prompt_enabled" = "1" ]; then
	prompt_enabled_val=true
else
	prompt_enabled_val=false
fi

su - ${current_user} -c "export XDG_RUNTIME_DIR=/run/user/${userid} \
&& export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${userid}/bus \
&&  gsettings set org.cdos.desktop.system.remote enabled ${is_allowd_remote_desktop_val}"
echo "set gsettings set org.cdos.desktop.system.remote enabled ${is_allowd_remote_desktop_val}" >> $log


su - ${current_user} -c "export XDG_RUNTIME_DIR=/run/user/${userid} \
&& export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${userid}/bus \
&&  gsettings set org.gnome.Vino prompt-enabled ${prompt_enabled_val}"
echo "set org.gnome.Vino prompt-enabled ${prompt_enabled_val}" >> $log

su - ${current_user} -c "export XDG_RUNTIME_DIR=/run/user/${userid} \
&& export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${userid}/bus \
&&  gsettings set org.gnome.Vino view-only ${view_only}"
echo "set org.gnome.Vino view-only ${view_only}" >> $log

su - ${current_user} -c "export XDG_RUNTIME_DIR=/run/user/${userid} \
&& export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${userid}/bus \
&&  gsettings set org.gnome.Vino require-encryption false"
echo "set org.gnome.Vino require-encryption false" >> $log


if [ "$is_need_pwd" = "1" ]; then
	su - ${current_user} -c "export XDG_RUNTIME_DIR=/run/user/${userid} \
	&& export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${userid}/bus \
	&&  gsettings set org.gnome.Vino authentication-methods '[\"vnc\"]' "

	su - ${current_user} -c "export XDG_RUNTIME_DIR=/run/user/${userid} \
	&& export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${userid}/bus \
	&&  gsettings set org.gnome.Vino vnc-password '${remote_pwd}' "

	echo "set  authentication methods to vnc and set password" >> $log

else
	su - ${current_user} -c "export XDG_RUNTIME_DIR=/run/user/${userid} \
	&& export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${userid}/bus \
	&&  gsettings set org.gnome.Vino authentication-methods '[\"none\"]' "
	echo "set  authentication methods to none" >> $log
fi

exit 0
 
 
