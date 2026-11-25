#! /bin/bash

# kill x11vnc
# unnecessary to kill x11vnc
#pids=$(ps aux | grep /usr/bin/x11vnc | grep -v grep | awk '{print $2}')
#if [ -n "$pids" ]; then
	#kill -9 $pids
#fi

# kill novnc
pids=$(ps aux | grep /usr/share/novnc | grep -v grep | awk '{print $2}')
if [ -n "$pids" ]; then
	#kill -9 $pids shget error
	kill -15 $pids
fi

