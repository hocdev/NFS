#! /bin/bash

# stop vino vnc server

log=/var/log/nfs-domain-client/vino.log
touch $log
chmod 666 $log
echo "stop vino vnc server" >> $log
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
 
 
su - ${current_user} -c "export XDG_RUNTIME_DIR=/run/user/${userid} \
&& export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${userid}/bus \
&&  systemctl --user stop vino-server.service"

echo "vino-server stop" >> $log

exit 0
