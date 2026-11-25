#!/bin/bash

LOCAL_IP=$1
SERVER_ADDR=$2
HOSTNAME=$3
MINION_ID_PATH=/usr/share/nfs-domain-client/salt/minion_id
MINION_PATH=/usr/share/nfs-domain-client/salt/minion
MASTER_PUB=/etc/salt/pki/minion/minion_master.pub
MASTER_PUB2=/var/lib/salt/pki/minion/minion_master.pub
MASTER_CACHE=/var/cache/salt

systemctl stop salt-minion
sleep 5
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

rm -rf /var/lib/salt/pki/minion/

echo $LOCAL_IP > $MINION_ID_PATH
sed -i "/^ master: / s/.*/ master: $SERVER_ADDR/" $MINION_PATH
sed -i "/^ id: / s/.*/ id: $HOSTNAME/" $MINION_PATH

cp "$MINION_ID_PATH" /etc/salt/minion_id
cp "$MINION_PATH" /etc/salt/minion
chmod 666 /usr/share/nfs-domain-client/salt/config.ini

systemctl enable salt-minion
systemctl restart salt-minion

exit 0