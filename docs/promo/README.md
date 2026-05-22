# CSD 宣传页

这是 CSD（C-Drive Saver）项目的官方宣传页面，用于展示项目特性、性能指标和技术栈。

## 文件结构

```
docs/promo/
├── index.html          # 主 HTML 文件
├── styles/
│   └── main.css        # 样式文件
└── README.md           # 说明文档（本文件）
```

## 使用方式

### 本地预览

直接在浏览器中打开 `index.html` 文件即可预览。

### 部署到 Cloudflare Pages (推荐)

此页面经过优化，已经完全自包含，非常适合通过 Cloudflare Pages 与 GitHub 自动联动。每次您推送代码至 GitHub，页面都会自动更新。

#### 1. 提交并推送代码
确保您已将本地最新的修改（包括自包含的 `assets/` 文件夹及更新后的 `index.html`）提交并推送至 GitHub 仓库。

#### 2. 在 Cloudflare Pages 中配置
1. 登录 [Cloudflare 控制台](https://dash.cloudflare.com/)。
2. 导航至 **Workers & Pages (Workers 和 Pages)** -> 点击 **Create (创建)** -> 选择 **Pages** 标签页 -> 点击 **Connect to Git (连接到 Git)**。
3. 授权并选择您的 GitHub 仓库 `bjfwan/cdrive-cleaner`。
4. 在 **Set up builds and deployments (设置构建和部署)** 页面中进行如下配置：
   - **Project name (项目名称)**：自定义（如 `cdrive-cleaner` 或 `csd`）。
   - **Production branch (生产分支)**：选择您的默认分支（如 `main` 或 `master`）。
   - **Framework preset (框架预设)**：选择 **None**。
   - **Build command (构建命令)**：**留空**（无需任何构建命令）。
   - **Build output directory (构建输出目录)**：填写 `docs/promo` ⚠️ *非常关键，这会把此子目录作为网站根目录。*
   - **Root directory (根目录)**：**留空**。
5. 点击 **Save and Deploy (保存并部署)**。

#### 3. 绑定您的域名
1. 部署完成后，进入该 Pages 项目的控制台。
2. 选择 **Custom domains (自定义域)** 选项卡 -> 点击 **Set up a custom domain (设置自定义域)**。
3. 输入您在 Cloudflare 上的域名（如 `csd.yourdomain.com`），点击继续。
4. Cloudflare 会自动帮您在 DNS 中配置 CNAME 解析并申请 SSL 证书。
5. 绑定成功后，即可通过您的域名访问！此后每次 `git push`，该宣传发布页就会秒级自动完成更新。

## 内容模块

页面包含以下主要模块：

1. **Hero 区域** - 项目标题、简介、下载按钮
2. **统计数据** - 性能关键指标展示
3. **核心功能** - 6 大核心功能介绍
4. **迁移流程** - 5 步迁移流程可视化
5. **安全等级** - 4 级安全检测说明
6. **性能指标** - 详细性能基准数据
7. **技术栈** - 使用的技术和工具
8. **CTA 区域** - 行动号召和下载链接

## 自定义

### 修改版本号

在 `index.html` 中找到：

```html
<span>v0.1.8 · 开源免费 · GPL-3.0</span>
```

### 修改颜色主题

在 `styles/main.css` 的 `:root` 部分修改 CSS 变量：

```css
:root {
  --accent: #6366f1;        /* 主色调 */
  --accent-light: #818cf8;  /* 浅色调 */
  --green: #22c55e;         /* 成功色 */
  --amber: #f59e0b;         /* 警告色 */
  --rose: #f43f5e;          /* 危险色 */
  --cyan: #06b6d4;          /* 信息色 */
}
```

### 修改性能数据

在 `index.html` 的统计数据和性能指标部分更新数值。

## 依赖

- Google Fonts (Inter + Noto Sans SC) - 通过 CDN 加载
- 项目图标 - 引用自 `assets/app-icon.png`

## 浏览器兼容性

- Chrome/Edge 90+
- Firefox 88+
- Safari 14+

## 截图用途

此页面设计为 1200px 宽度，适合用于：

- 项目宣传截图
- 社交媒体分享
- 文档配图
- VibeCoding 大赏投稿素材

## 维护

更新内容时请同步修改：

1. 版本号
2. 性能数据
3. 功能描述
4. GitHub 链接

## License

与主项目保持一致：GPL-3.0
