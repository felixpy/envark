import { useState } from 'react'
import { ArrowUp, Check, Download, FilePenLine, Plus, Star, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import {
  emptyProvider,
  formatBytes,
  metadata,
  updateKind,
  type ConfigContent,
  type ProviderId,
  type Tool,
} from '@/domain'
import { useStore } from '@/store'
import { EcoDot, Empty, SearchInput } from '@/components/shared'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { capabilities } from './Catalog'
import { Checkbox } from '@/components/ui/checkbox'
import { useSelection } from '@/hooks/use-selection'

export default function Environments({ id }: { id: ProviderId }) {
  const s = useStore()
  const { t } = s
  const meta = metadata[id]
  const provider = s.data.inventory.providers.find((p) => p.id === id) ?? emptyProvider(id)
  const caps = capabilities(id)
  const [tab, setTab] = useState(caps[0].id)
  const [install, setInstall] = useState(false)
  const preferred = s.data.settings.preferred[id] ?? provider.managers[0]?.name ?? ''
  const [manager, setManager] = useState(preferred)
  const [version, setVersion] = useState('')
  const [download, setDownload] = useState(false)
  const [model, setModel] = useState('')
  const [config, setConfig] = useState<ConfigContent | null>(null)
  const [saving, setSaving] = useState(false)
  const assets = useSelection(
    provider.assets
      .filter((asset) => asset.canRemove && asset.size.complete)
      .map((asset) => asset.id),
  )
  const operation = (
    kind: 'setDefault' | 'removeRuntime' | 'updateTool' | 'removeTool',
    itemId: string,
  ) => void s.prepare({ kind, provider: id, id: itemId })
  const editConfig = async (configId: string) => {
    try {
      setConfig(await s.api.readConfig(configId))
    } catch (error) {
      toast.error(String(error))
    }
  }
  const saveConfig = async () => {
    if (!config) return
    setSaving(true)
    try {
      setConfig(await s.api.saveConfig(config.id, config.content, config.revision))
      await s.reload()
      toast.success(
        t('配置已保存，原文件已备份。', 'Configuration saved. The previous file was backed up.'),
      )
    } catch (error) {
      toast.error(String(error))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <EcoDot id={id} className="size-2.5" />
            <h1 className="text-2xl font-semibold tracking-tight">{meta.name}</h1>
            <Badge variant="outline">
              {provider.detected ? t('已检测到', 'Detected') : t('未检测到', 'Not detected')}
            </Badge>
          </div>
          <p className="max-w-2xl text-sm text-muted-foreground">
            {t(meta.description[0], meta.description[1])}
          </p>
        </div>
        {provider.service && (
          <div className="rounded-lg border px-3 py-2 text-xs">
            <span
              className={`mr-2 inline-block size-1.5 rounded-full ${provider.service.running ? 'bg-emerald-500' : 'bg-muted-foreground'}`}
            />
            {provider.service.running
              ? t('服务运行中', 'Service running')
              : t('服务已停止', 'Service stopped')}
            <p className="mt-1 font-mono text-muted-foreground">{provider.service.endpoint}</p>
          </div>
        )}
      </div>
      {provider.issues.map((issue) => (
        <div
          key={issue}
          role="status"
          className="rounded-lg border bg-muted/40 p-3 text-xs text-muted-foreground"
        >
          {issue}
        </div>
      ))}
      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          {caps.map((cap) => (
            <TabsTrigger key={cap.id} value={cap.id}>
              <cap.icon className="size-4" />
              {t(cap.zh, cap.en)}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
      {tab === 'runtime' && (
        <Card className="shadow-none">
          <CardHeader>
            <CardTitle>
              {meta.runtime} {t('运行时', 'runtimes')}
            </CardTitle>
            <CardDescription>
              {provider.managers.length
                ? `${t('优先使用', 'Preferred manager')}: ${preferred}`
                : t(
                    '没有检测到可管理版本的工具。系统安装的运行时保持只读。',
                    'No supported version manager detected. System runtimes are read-only.',
                  )}
            </CardDescription>
            <CardAction>
              <Button
                size="sm"
                disabled={!provider.managers.some((m) => m.supportsInstall) || s.busy}
                onClick={() => {
                  setManager(preferred)
                  setInstall(true)
                }}
              >
                <Plus />
                {t('安装版本', 'Install version')}
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent className="space-y-4">
            {provider.managers.length > 0 && (
              <div className="grid grid-cols-2 gap-3 max-md:grid-cols-1">
                {provider.managers.map((item) => (
                  <div
                    key={item.name}
                    className={`flex items-center gap-3 rounded-lg border p-3 ${preferred === item.name ? 'border-foreground/30 bg-muted/40' : ''}`}
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2 text-sm font-medium">
                        <span className="font-mono">{item.name}</span>
                        {item.name === preferred && <Badge>{t('优先', 'Preferred')}</Badge>}
                      </div>
                      <div
                        className="truncate font-mono text-xs text-muted-foreground"
                        title={item.path}
                      >
                        {item.version}
                      </div>
                    </div>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={preferred === item.name || s.busy}
                      onClick={() =>
                        void s.saveSettings({
                          ...s.data.settings,
                          preferred: { ...s.data.settings.preferred, [id]: item.name },
                        })
                      }
                    >
                      {preferred === item.name ? <Check /> : <Star />}
                      {preferred === item.name
                        ? t('优先使用', 'Preferred')
                        : t('设为优先', 'Prefer')}
                    </Button>
                  </div>
                ))}
              </div>
            )}
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('版本', 'Version')}</TableHead>
                  <TableHead>{t('来源', 'Source')}</TableHead>
                  <TableHead className="text-right">{t('占用', 'Size')}</TableHead>
                  <TableHead className="text-right">{t('操作', 'Actions')}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {provider.runtimes.map((runtime) => (
                  <TableRow key={runtime.id}>
                    <TableCell>
                      <div className="flex items-center gap-2 font-mono">
                        {runtime.version}
                        {runtime.active && <Badge>{t('当前环境', 'Current environment')}</Badge>}
                        {!runtime.activeKnown && (
                          <Badge variant="outline">{t('活动状态未知', 'Activity unknown')}</Badge>
                        )}
                        {!runtime.managed && (
                          <Badge variant="outline">{t('只读', 'Read-only')}</Badge>
                        )}
                      </div>
                      <div
                        title={runtime.path}
                        className="mt-1 max-w-80 truncate font-mono text-xs text-muted-foreground"
                      >
                        {runtime.path}
                      </div>
                    </TableCell>
                    <TableCell className="text-muted-foreground">{runtime.manager}</TableCell>
                    <TableCell className="text-right font-mono text-xs">
                      {runtime.size ? formatBytes(runtime.size.bytes) : '—'}
                    </TableCell>
                    <TableCell>
                      <div className="flex justify-end gap-1">
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={
                            !runtime.managed ||
                            !provider.managers.some(
                              (manager) =>
                                manager.name === runtime.manager && manager.supportsDefault,
                            ) ||
                            s.busy
                          }
                          onClick={() => operation('setDefault', runtime.id)}
                        >
                          <Star />
                          {t('设为默认', 'Set default')}
                        </Button>
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={
                            runtime.active || !runtime.activeKnown || !runtime.managed || s.busy
                          }
                          onClick={() => operation('removeRuntime', runtime.id)}
                        >
                          <Trash2 />
                          {t('卸载', 'Remove')}
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            {!provider.runtimes.length && (
              <Empty>
                {t(
                  '没有检测到已安装版本。刷新环境或使用已有版本管理器安装。',
                  'No installed versions detected. Refresh or install through an existing version manager.',
                )}
              </Empty>
            )}
          </CardContent>
        </Card>
      )}
      {(tab === 'pm' || tab === 'global') && (
        <Tools
          key={tab}
          items={tab === 'pm' ? provider.packageManagers : provider.tools}
          global={tab === 'global'}
          onUpdate={(item) => operation('updateTool', item.id)}
          onBatch={(ids) => void s.prepare({ kind: 'updateTools', provider: id, ids })}
          onRemove={(item) => operation('removeTool', item.id)}
        />
      )}
      {tab === 'assets' && (
        <Card className="shadow-none">
          <CardHeader>
            <CardTitle>
              {id === 'ollama'
                ? t('本地模型', 'Local models')
                : t('浏览器下载', 'Browser downloads')}
            </CardTitle>
            <CardDescription>
              {t(
                '最后使用时间无法可靠获取时显示未知。共享文件的逻辑大小不等于实际可释放空间。',
                'Unknown last-use times remain unknown. Logical size can include shared files.',
              )}
            </CardDescription>
            {id === 'ollama' && (
              <CardAction>
                <Button
                  size="sm"
                  disabled={!provider.service?.running || s.busy}
                  onClick={() => setDownload(true)}
                >
                  <Download />
                  {t('下载模型', 'Download model')}
                </Button>
              </CardAction>
            )}
          </CardHeader>
          <CardContent>
            {assets.chosen.length > 0 && (
              <div className="mb-3 flex items-center justify-between rounded-lg bg-muted/50 p-3 text-sm">
                <span>
                  {assets.chosen.length} {t('个资源已选择', 'resources selected')}
                </span>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={s.busy}
                  onClick={() =>
                    void s.prepare({ kind: 'removeAssets', provider: id, ids: assets.chosen })
                  }
                >
                  <Trash2 />
                  {t('审阅移除', 'Review removal')}
                </Button>
              </div>
            )}
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-10">
                    <Checkbox
                      aria-label={t('选择全部资源', 'Select all resources')}
                      checked={
                        assets.checked ? true : assets.chosen.length ? 'indeterminate' : false
                      }
                      onCheckedChange={(on) => assets.toggleAll(on === true)}
                    />
                  </TableHead>
                  <TableHead>{t('资源', 'Resource')}</TableHead>
                  <TableHead>{t('最后使用', 'Last used')}</TableHead>
                  <TableHead className="text-right">{t('占用', 'Size')}</TableHead>
                  <TableHead />
                </TableRow>
              </TableHeader>
              <TableBody>
                {provider.assets.map((asset) => (
                  <TableRow key={asset.id}>
                    <TableCell>
                      <Checkbox
                        aria-label={`${t('选择', 'Select')} ${asset.name}`}
                        disabled={!asset.canRemove || !asset.size.complete || s.busy}
                        checked={assets.selected.has(asset.id)}
                        onCheckedChange={(on) => assets.toggle([asset.id], on === true)}
                      />
                    </TableCell>
                    <TableCell>
                      <div className="font-medium">
                        {asset.name}{' '}
                        <span className="font-mono text-xs text-muted-foreground">
                          {asset.version}
                        </span>
                      </div>
                      <div
                        className="mt-1 max-w-96 truncate font-mono text-xs text-muted-foreground"
                        title={asset.path}
                      >
                        {asset.path}
                      </div>
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {asset.lastUsed
                        ? new Date(asset.lastUsed * 1000).toLocaleDateString()
                        : t('未知', 'Unknown')}
                    </TableCell>
                    <TableCell className="text-right font-mono text-xs">
                      {asset.size.complete
                        ? formatBytes(asset.size.bytes)
                        : `≥ ${formatBytes(asset.size.bytes)}`}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={!asset.canRemove || !asset.size.complete || s.busy}
                        onClick={() =>
                          void s.prepare({ kind: 'removeAssets', provider: id, ids: [asset.id] })
                        }
                      >
                        <Trash2 />
                        {t('移除', 'Remove')}
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            {!provider.assets.length && (
              <Empty>{t('尚未检测到下载资源。', 'No downloaded resources detected.')}</Empty>
            )}
          </CardContent>
        </Card>
      )}
      {tab === 'config' && (
        <div className="space-y-3">
          {provider.configs.map((file) => (
            <Card key={file.id} className="shadow-none">
              <CardHeader>
                <CardTitle className="break-all font-mono text-sm">{file.path}</CardTitle>
                <CardDescription>
                  {file.format} ·{' '}
                  {t(
                    '保存前自动备份，并检查外部修改。',
                    'Backed up before saving; external changes are checked.',
                  )}
                </CardDescription>
                <CardAction>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={s.busy}
                    onClick={() => void editConfig(file.id)}
                  >
                    <FilePenLine />
                    {t('查看与编辑', 'View & edit')}
                  </Button>
                </CardAction>
              </CardHeader>
              {file.warning && (
                <CardContent className="text-sm text-amber-600">{file.warning}</CardContent>
              )}
            </Card>
          ))}
          {!provider.configs.length && (
            <Card>
              <Empty>
                {t(
                  '没有检测到支持的用户配置文件。',
                  'No supported user configuration files detected.',
                )}
              </Empty>
            </Card>
          )}
        </div>
      )}
      <Dialog open={install} onOpenChange={setInstall}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {t('安装', 'Install')} {meta.runtime}
            </DialogTitle>
            <DialogDescription>
              {t(
                '使用已有管理器安装指定版本，执行前可审阅具体操作。',
                'Install a version with an existing manager and review the operation before it runs.',
              )}
            </DialogDescription>
          </DialogHeader>
          <label className="space-y-2 text-sm">
            {t('版本管理器', 'Version manager')}
            <Select value={manager} onValueChange={setManager}>
              <SelectTrigger aria-label={t('版本管理器', 'Version manager')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {provider.managers
                  .filter((m) => m.supportsInstall)
                  .map((m) => (
                    <SelectItem key={m.name} value={m.name}>
                      {m.name}
                    </SelectItem>
                  ))}
              </SelectContent>
            </Select>
          </label>
          <label className="space-y-2 text-sm">
            {t('版本', 'Version')}
            <Input
              value={version}
              onChange={(e) => setVersion(e.target.value)}
              placeholder={id === 'rust' ? 'stable' : '22.11.0'}
            />
          </label>
          <DialogFooter>
            <Button
              disabled={!version.trim() || !manager}
              onClick={() => {
                setInstall(false)
                void s.prepare({
                  kind: 'installRuntime',
                  provider: id,
                  manager,
                  version: version.trim(),
                })
              }}
            >
              {t('审阅安装', 'Review installation')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={download} onOpenChange={setDownload}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('下载模型', 'Download model')}</DialogTitle>
            <DialogDescription>
              {t(
                '输入 Ollama 模型名称及标签，例如 llama3.2:3b。',
                'Enter an Ollama model and tag, for example llama3.2:3b.',
              )}
            </DialogDescription>
          </DialogHeader>
          <Input
            value={model}
            onChange={(e) => setModel(e.target.value)}
            aria-label={t('模型名称', 'Model name')}
            placeholder="llama3.2:3b"
          />
          <DialogFooter>
            <Button
              disabled={!model.trim()}
              onClick={() => {
                setDownload(false)
                void s.prepare({ kind: 'downloadAsset', provider: id, name: model.trim() })
              }}
            >
              {t('审阅下载', 'Review download')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog
        open={!!config}
        onOpenChange={(open) => {
          if (!open && !saving) setConfig(null)
        }}
      >
        <DialogContent className="sm:max-w-3xl">
          <DialogHeader>
            <DialogTitle>{t('编辑配置文件', 'Edit configuration')}</DialogTitle>
            <DialogDescription className="break-all font-mono text-xs">
              {config?.path}
            </DialogDescription>
          </DialogHeader>
          <Textarea
            className="min-h-80 font-mono text-xs"
            aria-label={t('配置内容', 'Configuration content')}
            value={config?.content ?? ''}
            onChange={(e) => setConfig(config ? { ...config, content: e.target.value } : null)}
          />
          <p className="text-xs text-muted-foreground">
            {t(
              '备份保存在应用数据目录中。配置内容可能包含访问凭据，请谨慎分享。',
              'Backups are stored in the app data directory. Configuration may contain credentials.',
            )}
          </p>
          <DialogFooter>
            <Button disabled={saving || !config} onClick={() => void saveConfig()}>
              {saving ? t('保存中', 'Saving') : t('备份并保存', 'Back up & save')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function Tools({
  items,
  global,
  onUpdate,
  onBatch,
  onRemove,
}: {
  items: Tool[]
  global: boolean
  onUpdate(item: Tool): void
  onBatch(ids: string[]): void
  onRemove(item: Tool): void
}) {
  const { t, busy } = useStore()
  const [query, setQuery] = useState('')
  const [status, setStatus] = useState('all')
  const [source, setSource] = useState('all')
  const [runtime, setRuntime] = useState('all')
  const visible = items.filter(
    (item) =>
      item.name.toLowerCase().includes(query.trim().toLowerCase()) &&
      (source === 'all' || item.source === source) &&
      (runtime === 'all' || (item.runtime ?? 'standalone') === runtime) &&
      (status === 'all' ||
        (status === 'outdated'
          ? ['major', 'minor'].includes(updateKind(item))
          : updateKind(item) === status)),
  )
  const selection = useSelection(visible.filter((item) => item.canUpdate).map((item) => item.id))
  return (
    <Card className="shadow-none">
      <CardHeader>
        <CardTitle>
          {global
            ? t('全局工具', 'Global tools')
            : t('包管理器与构建工具', 'Package managers & build tools')}
        </CardTitle>
        <CardDescription>
          {global
            ? t(
                '显示安装来源、绑定运行时和可用操作。',
                'Inspect installation owners, runtime bindings, and available actions.',
              )
            : t(
                '负责依赖解析与构建，更新方式取决于原始安装来源。',
                'Dependency and build tooling, managed by its original installer.',
              )}
        </CardDescription>
        {selection.chosen.length > 0 && (
          <CardAction>
            <Button size="sm" disabled={busy} onClick={() => onBatch(selection.chosen)}>
              <ArrowUp />
              {t('更新所选', 'Update selected')} ({selection.chosen.length})
            </Button>
          </CardAction>
        )}
      </CardHeader>
      <CardContent className="space-y-3">
        {global && (
          <div className="flex flex-wrap items-center gap-2">
            <SearchInput
              value={query}
              onChange={setQuery}
              placeholder={t('搜索工具', 'Search tools')}
            />
            <Select value={status} onValueChange={setStatus}>
              <SelectTrigger className="w-36" aria-label={t('全部状态', 'All statuses')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {[
                  ['all', t('全部状态', 'All statuses')],
                  ['outdated', t('可更新', 'Updatable')],
                  ['major', t('大版本更新', 'Major update')],
                  ['latest', t('已最新', 'Up to date')],
                  ['unknown', t('未检查', 'Not checked')],
                ].map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Select value={runtime} onValueChange={setRuntime}>
              <SelectTrigger className="ml-auto w-44" aria-label={t('全部运行时', 'All runtimes')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t('全部运行时', 'All runtimes')}</SelectItem>
                {[...new Set(items.map((item) => item.runtime ?? 'standalone'))].map((name) => (
                  <SelectItem key={name} value={name}>
                    {name === 'standalone' ? t('独立安装', 'Standalone') : name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Select value={source} onValueChange={setSource}>
              <SelectTrigger className="w-36" aria-label={t('全部来源', 'All sources')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t('全部来源', 'All sources')}</SelectItem>
                {[...new Set(items.map((item) => item.source))].map((name) => (
                  <SelectItem key={name} value={name}>
                    {name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        )}
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-10">
                <Checkbox
                  aria-label={t('选择全部可更新工具', 'Select all updatable tools')}
                  checked={
                    selection.checked ? true : selection.chosen.length ? 'indeterminate' : false
                  }
                  onCheckedChange={(on) => selection.toggleAll(on === true)}
                />
              </TableHead>
              <TableHead>{t('名称', 'Name')}</TableHead>
              <TableHead>{t('版本', 'Version')}</TableHead>
              <TableHead>{t('来源 / 运行时', 'Source / runtime')}</TableHead>
              <TableHead className="text-right">{t('操作', 'Actions')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {visible.map((item) => (
              <TableRow key={item.id}>
                <TableCell>
                  <Checkbox
                    aria-label={`${t('选择', 'Select')} ${item.name}`}
                    disabled={!item.canUpdate || busy}
                    checked={selection.selected.has(item.id)}
                    onCheckedChange={(on) => selection.toggle([item.id], on === true)}
                  />
                </TableCell>
                <TableCell>
                  <div className="font-medium">{item.name}</div>
                  <p
                    className="mt-1 max-w-64 truncate text-xs text-muted-foreground"
                    title={item.note ?? ''}
                  >
                    {item.note}
                  </p>
                </TableCell>
                <TableCell className="font-mono text-xs">
                  {item.version}
                  {item.latest && item.latest !== item.version && (
                    <span className="block text-emerald-600">→ {item.latest}</span>
                  )}
                </TableCell>
                <TableCell className="text-xs text-muted-foreground">
                  {item.source}
                  <span className="mt-1 block font-mono">
                    {item.runtime ?? t('独立安装', 'Standalone')}
                  </span>
                </TableCell>
                <TableCell>
                  <div className="flex justify-end gap-1">
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={!item.canUpdate || busy}
                      onClick={() => onUpdate(item)}
                    >
                      <ArrowUp />
                      {t('更新', 'Update')}
                    </Button>
                    {global && (
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={!item.canRemove || busy}
                        onClick={() => onRemove(item)}
                      >
                        <Trash2 />
                        {t('卸载', 'Remove')}
                      </Button>
                    )}
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        {!visible.length && (
          <Empty>{t('没有符合条件的工具。', 'No tools match these filters.')}</Empty>
        )}
      </CardContent>
    </Card>
  )
}
