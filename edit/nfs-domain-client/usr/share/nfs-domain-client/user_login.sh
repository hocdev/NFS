#!/usr/bin/expect

set logFile "/var/log/nfs-domain-client/user_login.log"
#log_file -a $logFile

# 设置超时时间, 兼顾部分性能不好的机器
set timeout 10
set username [lindex $argv 0]
set password [lindex $argv 1]

# 执行登录命令
spawn login $username
expect "*密码*"
send "$password\n"
expect "*assword*"
send "$password\n"

#log_file

# 等待命令执行完成
send "exit\n"
expect eof

