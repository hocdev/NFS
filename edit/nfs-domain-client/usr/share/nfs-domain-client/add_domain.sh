#! /bin/bash

# 用户输入等待超时时间
WAIT_TIME=900					
# 等待加域时间
WAIT_ADD_DOMAIN=141 #120s + 20s // 延时为3的倍数
PLATFORM=""
SERVER_IP=""
SERVER_PORT=""
DOMAIN_USER=""
PASSWD=""
HOSTNAME=""
DOMAIN_TYPE=$7
CONFIG_PATH=/usr/share/nfs-domain-client/salt/config.ini

# 与用户进行交互获取必要信息，准确性更高
get_interact_info () {
	local info
	local reply
	local flag=0
	local tip=$1

	while [ "$flag" -ne "1" ]
	do
		read -t $WAIT_TIME -p "$tip" info
		read -t $WAIT_TIME -p "Confirm '$info' is correct? [y/n]: " reply

		if [ "$reply" == "y" ] || [ "$reply" == "yes" ] || [ "$reply" == "Y" ] || [ "$reply" == "YES" ]; then
			echo $info
			flag=1
		fi
		echo $info
		flag=1
	done

}

# 平台信息
PLATFORM=$1
if [ $PLATFORM != "linux" ] && [ $PLATFORM != "window" ]
then
	echo "Invalid platform info!"
	exit 1
fi

# 服务端IP
echo " "
SERVER_IP=$2
# 检测 IP 地址的合法性
if [[ $SERVER_IP =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
	# 将 IP 地址拆分成 4 个字段
	IFS='.' read -r -a array <<< "$SERVER_IP"
	# 遍历每个字段，检查其值是否在 0-255 之间
	for i in "${array[@]}"; do
    		if (( $i < 0 || $i > 255 )); then
	      		echo "Invalid IP address: $SERVER_IP"
	      		exit 1
    		fi
  	done
else
	echo "Invalid IP address: $SERVER_IP"
	exit 1
fi

# 端口号
echo " "
SERVER_PORT=$3
# 检测端口号的合法性
if [[ $SERVER_PORT =~ ^[0-9]+$ ]]; then
 	# 检查端口号是否在 0-65535 之间
  	if (( $SERVER_PORT < 0 && $SERVER_PORT > 65535 )); then
		echo "Invalid port: $SERVER_PORT"
		exit 1
  	fi
else
  	echo "Invalid port: $SERVER_PORT"
  	exit 1
fi

# 域用户名
echo " "
DOMAIN_USER=$4
if [[ $DOMAIN_USER =~ ^[A-Za-z0-9]+$ ]]; then
	echo "Invalid domain user: $DOMAIN_USER"
	exit 1
fi

# 域用户密码
echo " "
PASSWD=$5
if [[ $PASSWD =~ ^[A-Za-z0-9-_=+/%]+$ ]]; then
	echo "Invalid domain password"
	exit 1
fi

# 主机名
echo " "
HOSTNAME=$6
if [[ $HOSTNAME =~ ^[a-z0-9.-]+$ ]]; then
	echo "Invalid hostname: $HOSTNAME"
	exit 1
fi


# TODO
# 加入window AD域
join_window_domain () {
	echo "Not supported yet!"
}

# 加入linux域
join_linux_domain () {
	# 使用仅管控模式
	if [ $DOMAIN_TYPE -eq 2 ]; then
		local_ip=`ip addr | grep 'state UP' -A2 | awk '/inet/ {print $2}' | cut -f1 -d'/' | head -n 1`

		# 启动salt-minion
		/usr/share/nfs-domain-client/salt/initsalt.sh ${local_ip} ${SERVER_IP} ${HOSTNAME}
		ret=$?
		if [ $ret -ne 0 ]; then
			echo "failed to init salt-minion"
			exit $ret
		fi

		# 记录信息到config.ini
		status_line=`grep -n "^add_domain_success=" $CONFIG_PATH |awk -F: '{print $1}'`
		sed -i "${status_line}"'d' $CONFIG_PATH
		sed -i "${status_line}i\add_domain_success=true" $CONFIG_PATH
		# 更新ip_port
		sed -i "s/^ip_port=.*/ip_port=${SERVER_IP}:${SERVER_PORT}/g" $CONFIG_PATH
		# update domain type
		sed -i "s/^domain_type=.*/domain_type=${DOMAIN_TYPE}/g" $CONFIG_PATH
		# 更新hostname
		sed -i "s/^hostname=.*/hostname=${HOSTNAME}/g" $CONFIG_PATH

		# 重启nfsdomain.service, 自动连接websocket
		# 客户端自主加域时，config.ini中提前写死ip_port,启动即自动websocket连接
		# systemctl restart nfsdomain
		exit 0
	fi

	result=`cat $CONFIG_PATH | grep -c "^add_domain_success=true"`
	if  [ $result -eq 1 ]
	then
		echo "Device is already domain-joined; leave domain first!"
		exit 1
	fi

	dbus-send --print-reply --system --type=method_call --dest=com.nfs.domain  /com/domain/agent com.nfs.domain.agent.add_domain2 string:"$SERVER_IP:$SERVER_PORT" string:"$DOMAIN_USER" string:"$PASSWD" string:"${HOSTNAME}"
	time=0
	while true
	do
		result=`cat $CONFIG_PATH | grep -c "^add_domain_success=true"`
		if  [ $result -ne 0 ]
		then
			SALT_CHECK=$(ps -ef |grep salt-minion|grep -v grep|wc -l)
			if [ "${SALT_CHECK}" != "0" ] ;then
				systemctl restart salt-minion.service
				echo "Restarting salt-minion"
			fi

			echo "Domain join succeeded!"
			exit 0
		fi
		
		echo "Joining domain, please wait..."
		sleep 3
		let time=$time+3
		if [ $time -eq $WAIT_ADD_DOMAIN ]
		then
			echo "Domain join failed!"
			exit 1
		fi
	done
}

case $PLATFORM in
	linux)
		join_linux_domain
		;;
	window)
		join_window_domain
		;;
	*)
		echo "Please enter a valid platform type!"
		exit 1
		;;
esac

exit 0
