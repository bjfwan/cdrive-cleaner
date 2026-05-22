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

### 部署

可以将整个 `promo` 文件夹部署到任何静态网站托管服务：

- GitHub Pages
- Cloudflare Pages
- Vercel
- Netlify

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
- 项目图标 - 引用自 `../../src/assets/app-icon.png`

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
