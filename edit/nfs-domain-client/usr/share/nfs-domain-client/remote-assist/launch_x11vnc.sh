#! /bin/bash

# 根据规则启动x11vnc

log=/var/log/nfs-domain-client/x11vnc.log
touch $log
chmod 666 $log

user=`who | grep "(:"  | awk '{print $1}' | head -n 1`
if [ -z "$user" ]; then
	echo "error: no user login" >> $log
	exit -1
fi
echo "==========user:" $user >> $log

# kill x11vnc
#pids=$(ps aux | grep /usr/bin/x11vnc | grep -v grep | awk '{print $2}')
#if [ -n "$pids" ]; then
#	kill -9 $pids
#fi
q
# kill novnc
pids=$(ps aux | grep /usr/share/novnc | grep -v grep | awk '{print $2}')
if [ -n "$pids" ]; then
	#kill -9 $pids shget error
	kill -15 $pids
fi

# -display :0 默认连接第一个显示屏
# -nopw  连接不用输入密码
# -accept popup 连接时弹窗提示
# -rfbport 5900 设置监听端口5900
# -ncache 10 设置缓存帧
# -o $log 设置日志输出文件
# -clipboard 将剪切板共享给远程客户端
# -shared
launch_x11vnc() {
	current_user=`logname`
	if [ -z "$current_user" ]; then
		current_user=`who | grep "(:0"  | awk '{print $1}'`
		if [ -n "$current_user" ]; then
			su - $current_user -c "/usr/bin/x11vnc  -forever -display :0  -shared -nopw -bg  -rfbport 5900 -o $log &"
		else
			current_user=`who | grep "(:1" | awk '{print $1}'`
			if [ -n "$current_user" ]; then
				su - $current_user -c "/usr/bin/x11vnc  -forever -display :1  -shared -nopw -bg  -rfbport 5900 -o $log &"
			else
				echo "error: no 0,1 user login" >> $log
				exit -1
			fi
		fi
	else
		displayid=`who  | grep "(:" |grep $current_user  | awk '{print $NF}'`
		# (:1),获取括号中内容":1"
		displayid=$(echo "$displayid" | sed -n 's/.*(//; s/)//p')
		su - $current_user -c "/usr/bin/x11vnc  -forever -display ${displayid}  -shared -nopw -bg  -rfbport 5900 -o $log &"
	fi
	echo "==========current_user is:" $current_user >> $log
}

# 启动x11vnc服务
existed=$(pidof x11vnc | wc -l)
if [ $existed -eq 0 ]; then
	#su - $current_user -c "/usr/bin/x11vnc  -forever -display :0  -shared -nopw -bg  -rfbport 5900 -o $log"
	launch_x11vnc
fi

