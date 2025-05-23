#!/bin/bash

SOURCE_DIR="../src/Figures"
TARGET_DIR="../src/Figures/PDF"
EXTS=(png jpg jpeg gif tif tiff bmp svg)

mkdir -p "$TARGET_DIR"

# 转换图片为PDF
for ext in "${EXTS[@]}"; do
    find "$SOURCE_DIR" -maxdepth 1 -type f -name "*.$ext" | while IFS= read -r img; do
        filename=$(basename -- "$img")
        name="${filename%.*}"
        pdf="$TARGET_DIR/$name.pdf"

        if [ ! -f "$pdf" ] || [ "$img" -nt "$pdf" ]; then
            echo "正在转换 $filename 为PDF..."
            convert "$img" "$pdf"
        fi
    done
done

# 删除没有源图的PDF
find "$TARGET_DIR" -maxdepth 1 -type f -name "*.pdf" | while IFS= read -r pdf; do
    name=$(basename -- "$pdf" .pdf)

    found=0
    for ext in "${EXTS[@]}"; do
        if [ -f "$SOURCE_DIR/$name.$ext" ]; then
            found=1
            break
        fi
    done

    if [ $found -eq 0 ]; then
        echo "源图片已删除，正在删除PDF: $name.pdf"
        rm "$pdf"
    fi
done

echo "图片转换完成"