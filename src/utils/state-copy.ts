export interface StateCopy {
  empty: { title: string; description: string; cta?: string };
  loading: { title: string; description: string };
  error: { title: string; description: string; retry?: string };
}

export const stateCopy: Record<string, StateCopy> = {
  junk: {
    empty: {
      title: '你这台机器很干净',
      description: '没找到能安全清理的临时垃圾。可以稍后再扫，或者去看看大文件和重复文件。',
      cta: '查看大文件',
    },
    loading: {
      title: '正在翻找垃圾文件…',
      description: '会扫 Windows 系统临时目录、浏览器缓存、应用日志等位置。第一次大约 5–20 秒。',
    },
    error: {
      title: '扫描中断',
      description: '可能是某个目录权限不够。可以点重试，或在设置里把它加入跳过列表。',
      retry: '重新扫描',
    },
  },

  largeFiles: {
    empty: {
      title: '没有大文件',
      description: '当前阈值下没有任何超大文件。可以把阈值调低，或者切换到其他视图。',
      cta: '调低阈值',
    },
    loading: {
      title: '正在加载大文件…',
      description: '首屏先加载前 200 条，往下滚会继续读取。',
    },
    error: {
      title: '大文件列表加载失败',
      description: '可能是扫描结果缓存损坏。重新跑一次深度扫描通常能解决。',
      retry: '重新扫描',
    },
  },

  duplicates: {
    empty: {
      title: '没有发现重复文件',
      description: '在 ≥ 100 MB 的大文件里没找到内容完全一样的副本。',
    },
    loading: {
      title: '正在比对文件指纹…',
      description: '先按大小预筛，再做哈希比对。大盘可能需要 30 秒到几分钟。',
    },
    error: {
      title: '重复文件分析失败',
      description: '可能是某些文件被进程占用读不到。可以关掉相关应用后再试。',
      retry: '重新分析',
    },
  },

  games: {
    empty: {
      title: '没检测到游戏库',
      description: '请确保 Steam / Epic / Xbox 启动器至少完成过一次登录，再点上面的「重新检测」。',
      cta: '重新检测',
    },
    loading: {
      title: '正在扫描三家平台…',
      description: '读 Steam libraryfolders、Epic manifests、Xbox 已注册应用清单。',
    },
    error: {
      title: '游戏库检测失败',
      description: '可能是注册表权限不够，或者启动器配置文件损坏。先关掉游戏启动器再试一次。',
      retry: '重新检测',
    },
  },

  folderRedirect: {
    empty: {
      title: '没有可重定向的文件夹',
      description: '所有「下载/文档/图片/桌面」都已经在 C 盘以外，或者你只装了 C 一个盘。',
    },
    loading: {
      title: '正在检测已知文件夹…',
      description: '读 Windows 注册表里的 Shell Folders，看每个文件夹当前指向哪个盘。',
    },
    error: {
      title: '检测失败',
      description: '注册表读不到。请确认应用以管理员身份运行，再重试。',
      retry: '重新检测',
    },
  },

  systemReclaim: {
    empty: {
      title: '系统已经很精简了',
      description: '影子副本、休眠文件、Windows.old 都没什么可回收的。',
    },
    loading: {
      title: '正在扫描系统占用…',
      description: '检查影子副本、休眠文件、Windows.old、Update 残留这几个大头。',
    },
    error: {
      title: '系统占用扫描失败',
      description: '部分系统区域需要管理员权限才能读。请用管理员模式重启 CSD。',
      retry: '重新扫描',
    },
  },

  aiSuggestions: {
    empty: {
      title: '还没有 AI 建议',
      description: '在「设置 → AI 服务」里启用 AI 分析，然后跑一次扫描，建议会出现在这里。',
      cta: '去开启 AI',
    },
    loading: {
      title: '正在生成建议…',
      description: '把脱敏后的快照发给模型，等返回结构化建议。一般 5–15 秒。',
    },
    error: {
      title: 'AI 建议生成失败',
      description: '可能是网络问题，或者 BYOK 配置不对。可以打开「发送前预览」检查快照。',
      retry: '重新请求',
    },
  },

  workspace: {
    empty: {
      title: '选择一个磁盘开始',
      description: '点左侧栏的磁盘卡片，再点右上角「开始扫描」。',
      cta: '开始扫描',
    },
    loading: {
      title: '正在扫描磁盘…',
      description: '第一次会建立完整索引，之后再扫只读变化部分，会快很多。',
    },
    error: {
      title: '扫描失败',
      description: '可以重新扫描；如果反复失败，去设置里启用「管理员模式」试试。',
      retry: '重新扫描',
    },
  },

  history: {
    empty: {
      title: '还没有迁移历史',
      description: '在主视图里发起一次迁移后，记录会出现在这里，方便你随时回滚。',
    },
    loading: {
      title: '正在加载历史记录…',
      description: '读本地数据库里的迁移与清理日志。',
    },
    error: {
      title: '历史加载失败',
      description: '历史数据库可能损坏。可以在设置 → 缓存管理里看是否需要清掉重建。',
      retry: '重新加载',
    },
  },
};

export type ViewKey = keyof typeof stateCopy;
