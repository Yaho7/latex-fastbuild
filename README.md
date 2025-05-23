  ## 📖 选择语言 | Select Language
  
  - [English](./README.en.md)
  - [简体中文](./README.md)
  
  # 如何大幅提高 LaTeX 的编译速度,适用于MacOS（开源、傻瓜级教程）

  ## 前言

  大约两个月前，我开始系统地撰写论文，并在多种排版工具中选择了 LaTeX。但随着文稿逐渐变长、图片增多，编译速度越来越慢——特别是当插入大量 PNG、JPEG 图片时，每次编译往往需要 60 秒甚至更久。

  这是因为 LaTeX 在编译过程中需要将这些图片格式压缩、转换为 PDF 后再进行排版处理，极大拖慢了速度。

  通过本教程的设置，你可以将完整论文的编译时间 **压缩至 5 秒以内**。

  本教程分为两个阶段：

  - 配置 VS Code 的 `settings.json`
    - 设置 `latexmk` 编译工具
    - 配置图片自动转换 PDF 的脚本


  # 实现步骤
  0. 安装ImageMagick工具,请看文末附录
  1. 克隆本项目 
   - git clone https://github.com/Yaho7/latex-fastbuild.git
  2. 安装依赖
   - 确保本地已安装 ImageMagick（Mac 上可使用 brew install imagemagick）
   - 安装好 LaTeX 及 VS Code
  3. 在 VS Code 中打开本项目(PaperTemplate)
   - 默认已配置好 settings.json
   - 打开并编写 manuscript.tex
  4. 选择编译配方
   - 在 VS Code 中右键选择 “使用 LaTeX Workshop 编译”，或从命令面板选择 latexmk 配方


  ## 实现原理

  ### 1. 使用 `latexmk` 进行智能编译

  相比传统的 `xelatex` 手动编译方式，`latexmk` 具有以下优势：

  - **自动多轮编译**：自动检测 `.aux`、`.toc`、`.bbl` 等文件的变化，并自动调用编译器多次，直到输出稳定。
  - **自动处理参考文献**：当引用发生变化时，会自动运行 BibTeX 或 Biber。
  - **只编译必要部分**：如果部分内容未更改，它不会重复编译，从而显著提升速度。

  ### 2. 图片转换为 PDF 并长期缓存

  我们通过一个自定义脚本自动完成以下逻辑：

  - 编译时检查图片是否已有对应 PDF 文件；
  - 如有，则跳过；
  - 如无，则自动将 PNG、JPEG 等格式转换为 PDF 并保存；
  - 转换结果长期缓存，后续编译无需重复处理，显著加快速度。


# 附录

## **ImageMagick 安装教程**

### ** macOS 系统**

#### **使用 Homebrew 安装（推荐）**

1. 打开 Terminal（终端）；

2. 输入以下命令安装：

   ```
   brew install imagemagick
   ```

3. 验证是否安装成功：

   ```
   convert -version
   ```

   