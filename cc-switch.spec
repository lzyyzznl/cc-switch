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
mkdir -p %{buildroot}/usr/share/icons/hicolor/{48x48,128x128,256x256,512x512}/apps

# 安装二进制
install -m 755 cc-switch-server %{buildroot}/opt/cc-switch/

# 安装启动器脚本
install -m 755 cc-switch %{buildroot}/opt/cc-switch/

# 安装前端文件
install -m 644 frontend/index.html %{buildroot}/opt/cc-switch/frontend/
install -m 644 frontend/assets/* %{buildroot}/opt/cc-switch/frontend/assets/

# 安装版本文件
echo "%{version}-%{release}" > %{buildroot}/opt/cc-switch/VERSION

# 安装桌面文件和图标（多尺寸高清）
install -m 644 cc-switch.desktop %{buildroot}/usr/share/applications/
install -m 644 cc-switch-48.png  %{buildroot}/usr/share/icons/hicolor/48x48/apps/cc-switch.png
install -m 644 cc-switch-128.png %{buildroot}/usr/share/icons/hicolor/128x128/apps/cc-switch.png
install -m 644 cc-switch-256.png %{buildroot}/usr/share/icons/hicolor/256x256/apps/cc-switch.png
install -m 644 cc-switch.png     %{buildroot}/usr/share/icons/hicolor/512x512/apps/cc-switch.png

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

# 为每个已有用户创建桌面图标
for home_dir in /home/*; do
    [ -d "$home_dir" ] || continue
    user=$(basename "$home_dir")
    uid=$(id -u "$user" 2>/dev/null) || continue
    # 遍历常见桌面目录名
    for desktop_subdir in Desktop 桌面; do
        desktop_path="$home_dir/$desktop_subdir"
        if [ -d "$desktop_path" ]; then
            cp /usr/share/applications/cc-switch.desktop "$desktop_path/"
            chown "$uid" "$desktop_path/cc-switch.desktop"
            chmod 755 "$desktop_path/cc-switch.desktop"
        fi
    done
done

%preun
if [ "$1" -eq 0 ]; then
    # 真正卸载（非升级）时清理
    # 杀进程（PID 文件 + 进程名 + 端口全清理）
    if [ -f /tmp/cc-switch.pid ]; then
        kill "$(cat /tmp/cc-switch.pid)" 2>/dev/null || true
    fi
    pkill -f cc-switch-server 2>/dev/null || true
    fuser -k 10245/tcp 2>/dev/null || true
    sleep 1
    rm -f /tmp/cc-switch.lock /tmp/cc-switch.pid

    # 删除所有用户的桌面图标
    for home_dir in /home/*; do
        [ -d "$home_dir" ] || continue
        rm -f "$home_dir/Desktop/cc-switch.desktop"
        rm -f "$home_dir/桌面/cc-switch.desktop"
    done
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
/usr/share/icons/hicolor/48x48/apps/cc-switch.png
/usr/share/icons/hicolor/128x128/apps/cc-switch.png
/usr/share/icons/hicolor/256x256/apps/cc-switch.png
/usr/share/icons/hicolor/512x512/apps/cc-switch.png

%changelog
* Wed May 20 2026 李泽宇 <lzy@zluck.com> - 3.15.0-1
- Initial RPM package for CC Switch server-only mode
