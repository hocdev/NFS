#! /bin/bash

# start vino vnc server

log=/var/log/nfs-domain-client/vino.log
touch $log
chmod 666 $log
echo "start vino vnc server" >> $log
now=`date "+%Y-%m-%d %H:%M:%S"`
echo  $now >>$log
 

current_user=`who | grep "(:0"  | awk '{print $1}'`
if [ -n "$current_user" ]; then
	echo "found user used :0" >> $log
else
	current_user=`who | grep "(:1" | awk '{print $1}'`
	echo "found user used :1" >> $log
fi

userid=$(id -u ${current_user})

echo "current  username:" $current_user ", userid:" $userid >> $log

tmp_file="/tmp/${current_user}_remote_access_flag"
if [ -e "$tmp_file" ]; then
	echo "is_allowd_remote_desktop is false  return">> $log 
    return
fi

# start vino vnc server
su - ${current_user} -c "export XDG_RUNTIME_DIR=/run/user/${userid} \
&& export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${userid}/bus \
&&  systemctl --user restart vino-server.service"

echo "vino-server start" >> $log
 
# set system setting remote  on 
# su - ${current_user} -c "export XDG_RUNTIME_DIR=/run/user/${userid} \
# && export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${userid}/bus \
# &&  gsettings set org.cdos.desktop.system.remote enabled true"

# echo "set system setting remote  on " >> $log
exit 0
