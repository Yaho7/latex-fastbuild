## 📖 选择语言 | Select Language

- [English](./README.en.md)
- [简体中文](./README.md)
  
# How to Significantly Improve LaTeX Compilation Speed (Open-source, Beginner-friendly Tutorial)

## Introduction

About two months ago, I began systematically writing a paper and chose LaTeX among various typesetting tools. However, as my manuscript grew longer and included more images, compilation speed became increasingly slow—especially when inserting numerous PNG and JPEG images, each compilation often taking 60 seconds or longer.

This is because LaTeX needs to compress and convert these image formats to PDF during compilation before typesetting, which significantly slows down the process.

With the setup in this tutorial, you can **reduce the complete paper compilation time to under 5 seconds**.

This tutorial is divided into two stages:

- Configuring VS Code's `settings.json`
  - Setting up the `latexmk` compilation tool
  - Configuring scripts for automatic image conversion to PDF

## Implementation Steps

0. Install ImageMagick tool (see appendix at the end)
1. Clone this project
   ```
   git clone https://github.com/Yaho7/latex-fastbuild.git
   ```
2. Install dependencies
   - Ensure ImageMagick is installed locally (on Mac use `brew install imagemagick`)
   - Install LaTeX and VS Code
3. Open this project (PaperTemplate) in VS Code
   - The settings.json is already configured by default
   - Open and edit manuscript.tex
4. Choose the compilation recipe
   - Right-click in VS Code and select "Build with LaTeX Workshop" or select the latexmk recipe from the command palette

## How It Works

### 1. Using `latexmk` for Smart Compilation

Compared to traditional `xelatex` manual compilation, `latexmk` offers the following advantages:

- **Automatic Multi-round Compilation**: Automatically detects changes in `.aux`, `.toc`, `.bbl` and other files, and automatically calls the compiler multiple times until output stabilizes.
- **Automatic Reference Processing**: When citations change, it automatically runs BibTeX or Biber.
- **Selective Compilation**: If parts of the content haven't changed, it won't recompile them, significantly improving speed.

### 2. Converting Images to PDF with Long-term Caching

Our custom script automatically handles the following logic:

- Checks if images already have corresponding PDF files during compilation
- If yes, skips them
- If no, automatically converts PNG, JPEG, and other formats to PDF and saves them
- Conversion results are cached long-term, eliminating the need for repeated processing in subsequent compilations, significantly speeding up the process

# Appendix

## **ImageMagick Installation Guide**

### **macOS**

#### **Using Homebrew (Recommended)**

1. Open Terminal
2. Enter the following command to install:
   ```
   brew install imagemagick
   ```
3. Verify successful installation:
   ```
   convert -version
   ```

### **Windows**

1. Go to the official website: https://imagemagick.org/script/download.php#windows
2. Download ImageMagick-7.x.x-64-Q16-HDRI.exe (or version suitable for your system)
3. During installation, make sure to:
   - Check *Add application directory to your system path*
   - Check *Install legacy utilities (e.g., convert)*
4. After installation, open Command Prompt and enter:
   ```
   convert -version
   ```