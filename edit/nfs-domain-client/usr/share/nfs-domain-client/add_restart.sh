#!/bin/bash

SSSD=/lib/systemd/system/sssd.service
CHRONY=/lib/systemd/system/chrony.service
SALT_MINION=/lib/systemd/system/salt-minion.service

if [ -f "${SSSD}" ];then
	systemctl stop ${SSSD}
	exist_restart=$(cat ${SSSD} | grep "Restart=always")
	exist_restartsec=$(cat ${SSSD} | grep "RestartSec=10")
	
	if [ ! -n "${exist_restart}" ];then
		sed -i 's#^Restart=.*#Restart=always#g' ${SSSD}
	fi
	
	if [ ! -n "${exist_restartsec}" ];then
		var=$(awk '/Restart=/{print NR}' ${SSSD})
		sed -i ''"${var}"'a\RestartSec=10' ${SSSD}
	fi
	
	systemctl start ${SSSD}
fi

if [ -f "${CHRONY}" ];then
	systemctl stop ${CHRONY}
	exist_restart=$(cat ${CHRONY} | grep "Restart=always")
	exist_restartsec=$(cat ${CHRONY} | grep "RestartSec=10")	
	line=$(awk '/Service/{print NR}' ${CHRONY})
	var=`expr $line + 1`
	
	if [ ! -n "${exist_restart}" ];then
		sed -i ''"${line}"'a\Restart=always' ${CHRONY}
	fi
	
	if [ ! -n "${exist_restartsec}" ];then
		sed -i ''"${var}"'a\RestartSec=10' ${CHRONY}
	fi
	
	systemctl start ${CHRONY}
fi

if [ -f "${SALT_MINION}" ];then
	systemctl stop ${SALT_MINION}
	exist_restart=$(cat ${SALT_MINION} | grep "Restart=always")
	exist_restartsec=$(cat ${SALT_MINION} | grep "RestartSec=10")
	line=$(awk '/Service/{print NR}' ${SALT_MINION})
	var=`expr $line + 1`
	
	if [ ! -n "${exist_restart}" ];then
		sed -i ''"${line}"'a\Restart=always' ${SALT_MINION}
	fi
	
	if [ ! -n "${exist_restartsec}" ];then
		sed -i ''"${var}"'a\RestartSec=10' ${SALT_MINION}
	fi
	
	systemctl start ${SALT_MINION}
fi

exit 0
