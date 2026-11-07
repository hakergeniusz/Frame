# Spec file for Frame — verified targets: x86_64, aarch64
Name:           frame
Version:        1.0.0
Release:        1%{?dist}
Summary:        Independent keyboard-driven media player

# Frame's own code and assets are MIT. The libmpv engine is dynamically
# linked from the distribution's mpv-libs package and is not part of this
# RPM; it stays under its own licence (see THIRD_PARTY_LICENSES.md).
License:        MIT
URL:            https://github.com/hakergeniusz/Frame
Source0:        %{url}/archive/refs/tags/v%{version}/frame-%{version}.tar.gz

BuildRequires:  rust
BuildRequires:  cargo
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1) >= 1.5
BuildRequires:  pkgconfig(mpv)
Requires:       gtk4%{?_isa}
Requires:       libadwaita%{?_isa}
Requires:       mpv-libs%{?_isa}

%description
Frame plays local video files, whole folders, and network URLs with a
keyboard-driven GTK 4 / libadwaita interface: playlists with shuffle, watch
history, multiple audio and subtitle tracks (including a secondary subtitle
track), subtitle styling, playback speed, A-B looping, chapter navigation,
picture adjustments, screenshots, and a statistics overlay.

Playback is powered by libmpv, dynamically linked and remaining under its
own licence.

%prep
%autosetup -p1

%build
cargo build --release --locked

%install
install -Dpm 0755 target/release/frame %{buildroot}%{_bindir}/frame

_app=io.github.hakergeniusz.Frame
install -Dpm 0644 "assets/${_app}.desktop" \
  %{buildroot}%{_datadir}/applications/${_app}.desktop
install -Dpm 0644 "assets/${_app}.metainfo.xml" \
  %{buildroot}%{_metainfodir}/${_app}.metainfo.xml
for size in 16x16 24x24 32x32 48x48 64x64 128x128 256x256 512x512; do
  install -Dpm 0644 "assets/icons/${size}/apps/${_app}.png" \
    %{buildroot}%{_datadir}/icons/hicolor/${size}/apps/${_app}.png
done
install -Dpm 0644 assets/icons/scalable/apps/${_app}.svg \
  %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/${_app}.svg
install -Dpm 0644 assets/icons/scalable/actions/frame-ab-loop-symbolic.svg \
  %{buildroot}%{_datadir}/icons/hicolor/scalable/actions/frame-ab-loop-symbolic.svg
install -Dpm 0644 assets/icons/scalable/actions/frame-chapters-symbolic.svg \
  %{buildroot}%{_datadir}/icons/hicolor/scalable/actions/frame-chapters-symbolic.svg

%files
%license LICENSE
%doc README.md
%{_bindir}/frame
%{_datadir}/applications/io.github.hakergeniusz.Frame.desktop
%{_metainfodir}/io.github.hakergeniusz.Frame.metainfo.xml
%{_datadir}/icons/hicolor/16x16/apps/io.github.hakergeniusz.Frame.png
%{_datadir}/icons/hicolor/24x24/apps/io.github.hakergeniusz.Frame.png
%{_datadir}/icons/hicolor/32x32/apps/io.github.hakergeniusz.Frame.png
%{_datadir}/icons/hicolor/48x48/apps/io.github.hakergeniusz.Frame.png
%{_datadir}/icons/hicolor/64x64/apps/io.github.hakergeniusz.Frame.png
%{_datadir}/icons/hicolor/128x128/apps/io.github.hakergeniusz.Frame.png
%{_datadir}/icons/hicolor/256x256/apps/io.github.hakergeniusz.Frame.png
%{_datadir}/icons/hicolor/512x512/apps/io.github.hakergeniusz.Frame.png
%{_datadir}/icons/hicolor/scalable/apps/io.github.hakergeniusz.Frame.svg
%{_datadir}/icons/hicolor/scalable/actions/frame-ab-loop-symbolic.svg
%{_datadir}/icons/hicolor/scalable/actions/frame-chapters-symbolic.svg

%changelog
* Tue Nov 03 2026 hakergeniusz <hakergeniusz@users.noreply.github.com> - 1.0.0-1
- Initial release of Frame, an independent keyboard-driven media player.
