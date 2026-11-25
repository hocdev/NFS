#! /bin/bash

# 检查服务是否正在运行
service_status=$(systemctl is-active nfsdomain.service 2>/dev/null)
# 检查服务是否设置为开机自启
enable_status=$(systemctl is-enabled nfsdomain.service 2>/dev/null)

if [ "$service_status" = "active" ] && [ "$enable_status" = "enabled" ]; then
  systemctl --user enable /lib/systemd/user/nfs-domain-session.service
  status=$(systemctl --user is-active nfs-domain-session.service)
  if [ "$status" = "inactive" ]; then
    systemctl --user start nfs-domain-session.service
  fi
  dbus-send --session --type=method_call --print-reply --dest=com.nfs.domain.session /com/domain/agent com.nfs.domain.agent.post_login
  #start watermark
  SCRIPT_PATH="/usr/bin/start_watermark.sh"
   
  if [ -f "$SCRIPT_PATH" ]; then
      "$SCRIPT_PATH"
  fi
    exit 0
else
  systemctl --user disable /lib/systemd/user/nfs-domain-session.service
  exit 1
fi
