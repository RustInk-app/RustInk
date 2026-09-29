cargo build --release

rm -rf dist
mkdir -p dist/lib
mkdir -p dist/share/glib-2.0
mkdir -p dist/share/icons

cp target/release/rustInk.exe dist/
cp -r src/ui/icons dist/

ldd dist/rustInk.exe | grep -i 'ucrt64' | awk '{print $3}' | xargs -I '{}' cp -n '{}' dist/

cp -r /ucrt64/lib/gdk-pixbuf-2.0 dist/lib/
find /ucrt64/lib/gdk-pixbuf-2.0 -name "libpixbufloader-svg.dll" -exec ldd {} \; | grep -i 'ucrt64' | awk '{print $3}' | xargs -I '{}' cp -n '{}' dist/

cp -r /ucrt64/share/glib-2.0/schemas dist/share/glib-2.0/

cp -r /ucrt64/share/icons/hicolor dist/share/icons/
cp -r /ucrt64/share/icons/Adwaita dist/share/icons/