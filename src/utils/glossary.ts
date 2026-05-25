export interface GlossaryEntry {
  term: string;
  plain: string;
  detail?: string;
}

export const GLOSSARY: Record<string, GlossaryEntry> = {
  systemReserved: {
    term: '系统储备',
    plain: 'Windows 自动给系统留的应急空间',
    detail: '即使你看磁盘显示"已用 X GB"，这部分空间不在你的文件里，但确实占着盘。',
  },
  shadowCopy: {
    term: '影子副本',
    plain: 'Windows 用于系统还原的历史快照',
    detail: '系统出问题时可以回到这些快照。占空间但能救你，删除前最好做一次完整备份。',
  },
  safeAfterAction: {
    term: 'SafeAfterAction',
    plain: '完成必要操作后可以安全清理',
    detail: '例如更新安装完后，旧的安装缓存就属于这一类。',
  },
  hashDuplicate: {
    term: '哈希重复',
    plain: '内容完全一样的副本',
    detail: '同样的文件被复制到多个位置。CSD 用文件内容的指纹判定，名字不同也照样能识别。',
  },
  linkMigrate: {
    term: '链接迁移',
    plain: '文件搬家但软件不需要重装',
    detail: '把文件移到其他盘，原位置留一个"快捷方式式"的链接，程序读旧路径时自动指向新位置。',
  },
  hotPath: {
    term: '热路径',
    plain: '近期高频访问的位置，挪走可能影响速度',
    detail: '比如正在用的浏览器配置目录。CSD 在迁移前会自动标记这类目录，建议留在原盘。',
  },
  mftUsn: {
    term: 'MFT + USN',
    plain: 'Windows 文件系统自带的"目录目录"',
    detail: '比挨个文件夹翻快得多。需要管理员权限才能用，CSD 会自动尝试，不行就走普通方式。',
  },
  recycleBin: {
    term: '回收站',
    plain: 'Windows 回收站，删了可以还原',
    detail: '清理时默认走回收站。除非你在设置里勾了"永久删除"，否则都可以从回收站找回。',
  },
  permanentDelete: {
    term: '永久删除',
    plain: '跳过回收站，直接从硬盘抹掉',
    detail: '不能从回收站还原。CSD 在执行这类动作时会弹独立的红色确认框。',
  },
  symlink: {
    term: '符号链接',
    plain: '看起来是文件，其实指向真正位置的快捷方式',
    detail: '系统层面的"转跳"，应用程序看到的路径还是原来那个。',
  },
  junction: {
    term: 'junction',
    plain: '目录级别的链接',
    detail: '常用来把游戏库挪到其他盘后让 Steam/Epic 继续认得到旧路径。',
  },
  knownFolder: {
    term: '已知文件夹',
    plain: 'Windows 给你预设的文件夹（下载、文档、桌面等）',
    detail: '改它们的"默认存储位置"以后，新文件会直接落到其他盘，不用每次手动选。',
  },
  deepScan: {
    term: '深度扫描',
    plain: '把整个盘的目录树读一遍并缓存',
    detail: '第一次需要几十秒到几分钟（取决于盘大小），完成后大文件/重复文件这些视图才能用。',
  },
  incrementalScan: {
    term: '增量扫描',
    plain: '只读上次扫描后变化过的部分',
    detail: '快很多。CSD 后台扫描默认就是增量。',
  },
  dryRun: {
    term: 'Dry Run',
    plain: '模拟执行一遍，不真删',
    detail: '能预先看到"会删什么、会跳过什么、会节省多少"，决策完再真做。',
  },
  riskSafe: {
    term: '安全',
    plain: '删除后大概率不会影响任何东西',
    detail: 'CSD 判定为"安全"的项目通常是临时文件、过期缓存、可自动重建的内容。',
  },
  riskCaution: {
    term: '注意',
    plain: '删了多半没事，但建议你看一眼',
    detail: '比如某个应用的日志、临时数据。如果你正在用相关功能，可能会丢失少量上下文。',
  },
  riskRisky: {
    term: '风险',
    plain: '删了可能影响某些功能',
    detail: '比如 Windows Update 残留。删除节省的空间会比较多，但可能影响后续更新。',
  },
  riskBlocked: {
    term: '禁清',
    plain: '系统保护，CSD 不会让你动',
    detail: '比如 WindowsApps 这类目录。需要走系统设置或者改注册表，本工具不直接处理。',
  },
  defaultSelected: {
    term: '默认勾选',
    plain: '建议你直接清理的项',
    detail: 'CSD 已经评估过低风险，默认就替你勾上了。如果不放心可以手动取消。',
  },
  byok: {
    term: 'BYOK',
    plain: 'Bring Your Own Key，自带 API Key',
    detail: '用你自己的 OpenAI 兼容服务，请求不经过我们的代理。',
  },
  builtinAi: {
    term: 'Builtin AI',
    plain: '走 CSD 内置代理，每天免费 50 次',
    detail: '我们的 Cloudflare Worker 转发给上游模型服务商。所有路径在本地脱敏后再发出。',
  },
};

export function lookupGlossary(key: string): GlossaryEntry | null {
  return GLOSSARY[key] ?? null;
}

export function lookupByTerm(term: string): GlossaryEntry | null {
  for (const entry of Object.values(GLOSSARY)) {
    if (entry.term === term) return entry;
  }
  return null;
}
