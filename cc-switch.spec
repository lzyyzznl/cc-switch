%define _rpmdir %{_topdir}
%define _srcrpmdir %{_topdir}

Name:       cc-switch
Version:    3.15.0
Release:    1%{?dist}
Summary:    All-in-One Assistant for Claude Code, Codex & Gemini CLI
License:    MIT
URL:        https://github.com/lzyyzznl/cc-switch
Source0:    cc-switch-%{version}.tar.gz
BuildArch:  x86_64

Requires:   libnotify, xdg-utils, curl

%description
CC Switch is an all-in-one assistant for Claude Code, Codex & Gemini CLI.
This package provides the web-based UI mode (server-only) with a desktop
launcher that starts the service and opens the browser.

%prep
%setup -q -n cc-switch-%{version}

%install
# 主程序目录
mkdir -p %{buildroot}/opt/cc-switch/frontend/assets
mkdir -p %{buildroot}/usr/share/applications
mkdir -p %{buildroot}/usr/share/icons/hicolor/128x128/apps

# 安装二进制
install -m 755 cc-switch-server %{buildroot}/opt/cc-switch/

# 安装启动器脚本
install -m 755 cc-switch %{buildroot}/opt/cc-switch/

# 安装前端文件
install -m 644 frontend/index.html %{buildroot}/opt/cc-switch/frontend/
install -m 644 frontend/assets/* %{buildroot}/opt/cc-switch/frontend/assets/

# 安装版本文件
echo "%{version}-%{release}" > %{buildroot}/opt/cc-switch/VERSION

# 安装桌面文件和图标
install -m 644 cc-switch.desktop %{buildroot}/usr/share/applications/
install -m 644 cc-switch.png %{buildroot}/usr/share/icons/hicolor/128x128/apps/

%post
# 安装/升级时：杀掉旧进程、清锁
if [ -f /tmp/cc-switch.pid ]; then
    kill "$(cat /tmp/cc-switch.pid)" 2>/dev/null || true
fi
fuser -k 10245/tcp 2>/dev/null || true
rm -f /tmp/cc-switch.lock /tmp/cc-switch.pid

# 刷新桌面数据库
gtk-update-icon-cache /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database 2>/dev/null || true

%preun
if [ "$1" -eq 0 ]; then
    # 真正卸载（非升级）时清理进程
    if [ -f /tmp/cc-switch.pid ]; then
        kill "$(cat /tmp/cc-switch.pid)" 2>/dev/null || true
    fi
    fuser -k 10245/tcp 2>/dev/null || true
    rm -f /tmp/cc-switch.lock /tmp/cc-switch.pid
fi

%files
%dir /opt/cc-switch/
/opt/cc-switch/cc-switch-server
/opt/cc-switch/cc-switch
%dir /opt/cc-switch/frontend/
/opt/cc-switch/frontend/index.html
/opt/cc-switch/frontend/assets/*
/opt/cc-switch/VERSION
/usr/share/applications/cc-switch.desktop
/usr/share/icons/hicolor/128x128/apps/cc-switch.png

%changelog
* Wed May 20 2026 李泽宇 <lzy@zluck.com> - 3.15.0-1
- Initial RPM package for CC Switch server-only mode
