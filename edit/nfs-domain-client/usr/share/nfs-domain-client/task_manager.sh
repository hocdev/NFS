#! /bin/bash

Type=$1
Arg2=$2
Arg3=$3

declare -g logfile
taskdir=/usr/share/nfs-domain-client/tasks

print_log() {
	content=$1
	echo $(date "+%Y-%m-%d %H:%M:%S") $content >> $logfile
}

add_source() {
	serverip=`grep ip_port /usr/share/nfs-domain-client/salt/config.ini | awk -F= '{print $2}' | awk -F: '{print $1}'`
	sourcefile=/etc/apt/sources.list.d/${serverip}.list
	valid=true
	if ! [[ ${serverip} =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
		valid=false
	else
		IFS='.' read -ra parts <<< "${serverip}"
		for part in "${parts[@]}"; do
			if (( part < 0 || part > 255 )); then
				valid=false
				break
			fi
		done

	fi
	if $valid; then
		print_log "IP ${serverip} is valid "
	else
		print_log "IP ${serverip} is invalid "
		return
	fi

	addr="deb [trusted=yes] http://${serverip}/aptly/server server main"
	num=$(grep -c "${serverip}.*server" ${sourcefile})
	if [ $num -eq 0 ]; then
		print_log "add source ${addr} to ${sourcefile}"
		echo ${addr} >> ${sourcefile}
	fi

	print_log `apt-get update`
}

case $Type in
	download)
		logfile=${Arg2}/download.log
		if [ -f ${logfile} ]; then
			rm ${logfile}
		fi
		add_source
		print_log "ready to do task(${Arg2}), download: ${Arg3}"
		cd ${Arg2}
		content=$(apt-get download ${Arg3})
		print_log $content
		ret=$?
		if [ ${ret} -eq 0 ]; then
			print_log "download task(${Arg2}) success"
		else
			print_log "failed to download task(${Arg2})"
		fi

		;;
	install)
		logfile=${Arg2}/install.log
		if [ -f ${logfile} ]; then
			rm ${logfile}
		fi
		print_log "ready to do task(${Arg2}, install package"
		while true; do
			 # 检查是否有 apt 相关进程
			apt_processes=$(ps aux | grep -E "apt|apt-get|dpkg" | grep -v grep)
			if [ -n "$apt_processes" ]; then
				print_log "apt process running, waiting 30s..."
				sleep 60
				break
			else
				break
			fi
		done

		cd ${Arg2}
		print_log "Starting installation..."
		apt-get install -y --reinstall ./*.deb 2>&1 | tee -a ${logfile} ${syslogfile}
		
		ret=$?
		if [ ${ret} -eq 0 ]; then
			print_log "install task(${Arg2}) success"
		else
			print_log "failed to install task(${Arg2})"
		fi

		;;
	*)
		echo "error type:${Type}"
		;;
esac


