#! /bin/bash

Action=$1
Product=$PRODUCT
Id_serial=$ID_SERIAL
Id_vendor_id=$ID_VENDOR_ID

USBStatusFile="/usr/share/nfs-domain-client/usb/usbstatus"
DBFile="/usr/share/nfs-domain-client/db/data.db"
Table_event="usb_event"
Table_init="usb_init"
Output="/tmp/record_usb_op.log"

init_usb_device()
{
	if sqlite3 $DBFile ".table $Table_init" | grep $Table_init; then
		# 清空
		sqlite3 $DBFile "DELETE FROM $Table_init"
	else
		# 创建
		sqlite3 $DBFile <<EOF
		CREATE TABLE $Table_init (id INTEGER PRIMARY KEY, 
						product varchar(128) NOT NULL,
						id_serial varchar(64) NOT NULL,
						manufactor varchar(256) DEFAULT NULL)
EOF
	fi

	# 获取所有USB设备的信息
	usb_info=$(lsusb)

	# 循环处理每个USB设备
	while read -r line
	do
		# 提取USB设备的厂商ID和产品ID
		id_vendor_id=$(echo $line | awk '{print $6}' | cut -d':' -f1)
		product_id=$(echo $line | awk '{print $6}' | cut -d':' -f2)

		# 获取USB设备的产品号、厂序列号、厂商
		product=$(udevadm info -q property -n /dev/bus/usb/$(echo $line | awk '{print $2 "/" $4}' | tr -d ':') | grep PRODUCT= | cut -d'=' -f2)
		id_serial=$(udevadm info -q property -n /dev/bus/usb/$(echo $line | awk '{print $2 "/" $4}' | tr -d ':') | grep ID_SERIAL= | cut -d'=' -f2)
		manufactor=`grep $id_vendor_id /var/lib/usbutils/usb.ids | sed "s/$id_vendor_id  //"`
		echo "init product=$product" >> $Output
		echo "init id_vendor_id=$id_vendor_id" >> $Output
		echo "init id_serial=$id_serial"  >> $Output
		echo "init manufactor=$manufactor" >> $Output
		echo "---------------" >> $Output

		# 将USB设备信息写入表usb_init
		sqlite3 $DBFile "INSERT INTO $Table_init (product, id_serial, manufactor) VALUES ('$product', '$id_serial', '$manufactor');"
	done <<< "$usb_info"	
}


init_db()
{
	sqlite3 $DBFile "create table if not exists usb_event(
				id INTEGER PRIMARY KEY,
				product varchar(128) NOT NULL,
				id_serial varchar(64) NOT NULL,
				action varchar(6) DEFAULT NULL,
				manufactor varchar(256) DEFAULT NULL,
				time varchar(128) DEFAULT NULL
				)"

}

# 判断usb是否禁用
usb_enabled()
{
	enabled=`cat $USBStatusFile`
	if [ $enabled -eq 1 ]
	then
		echo "true"
	else
		echo "false"
	fi
}

get_value_by_product()
{
	product=$1
	key=$2
	value=""

	sql="SELECT $key FROM $Table_event WHERE product='$product';"
	sqlite3 $DBFile "$sql" > /tmp/record_usb_product
	while IFS= read -r line
	do
		if [[ -n "$line" ]]; then
			value=$line
			break
		fi
	done < /tmp/record_usb_product

	if [ -z "$value" ]; then
		sql="SELECT $key FROM $Table_init WHERE product='$product';"
		sqlite3 $DBFile "$sql" > /tmp/record_usb_product
		while IFS= read -r line
		do
			if [[ -n "$line" ]]; then
				value=$line
				break
			fi
		done < /tmp/record_usb_product
	fi
		
	echo $value
}

main()
{
	init_db

	# USB禁用时不记录插拔状态
	osversion=`cat /etc/os-release | grep "^VERSION_ID=" | awk -F '=' '{print $2}'`
	usb_enabled=`usb_enabled`
	echo "usb enable status: $usb_enabled"  >> $Output
	
	if [ $osversion == '"5.0"' ]; then
		if [ $usb_enabled == "false" ]; then
			return
		fi
	elif [ $osversion == "4.0.0" ]; then
		if [ $usb_enabled == "true" ]; then
			return
		fi
	fi

	action=$Action
	product=$Product
	id_serial=$Id_serial
	id_vendor_id=$Id_vendor_id
	manufactor=""
	if [ -n "$id_vendor_id" ]; then
		manufactor=`grep $id_vendor_id /var/lib/usbutils/usb.ids | head -n 1 | sed "s/$id_vendor_id[[:space:]]*//" | sed "s/^[ \t]*//"`
	fi

	time=$(date +"%Y-%m-%d %H:%M:%S")

	if [ $action == "remove" ]; then
		# 修改id_serial/manufactor
		id_serial=`get_value_by_product "$product" "id_serial"`
		manufactor=`get_value_by_product "$product" "manufactor"`
	fi
	
	echo "event trigger action=$action:" >> $Output
	echo "event product=$product" >> $Output
	echo "event id_vendor_id=$id_vendor_id" >> $Output
	echo "event id_serial=$id_serial" >> $Output
	echo "event manufactor=$manufactor" >> $Output

	sql="INSERT INTO usb_event(product, id_serial, action, manufactor, time) VALUES('$product', '$id_serial', '$action', '$manufactor', '$time');"
	sqlite3 $DBFile "$sql"
}

if [ $Action == "init" ]; then
	# 系统启动时初始化usb信息
	init_usb_device
elif [ $Action == "add" -o $Action == "remove" ]; then
	# 执行记录操作
	main
fi


