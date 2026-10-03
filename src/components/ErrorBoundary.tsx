import { Component, type ReactNode } from 'react'
import { AlertTriangle } from 'lucide-react'
import { Button } from './ui/button'

export class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state: { error: Error | null } = { error: null }

  static getDerivedStateFromError(error: Error) {
    return { error }
  }

  render() {
    if (!this.state.error) return this.props.children
    return (
      <main className="flex min-h-screen items-center justify-center bg-background p-8 text-foreground">
        <div role="alert" className="max-w-lg space-y-4 rounded-xl border p-6">
          <AlertTriangle className="size-6 text-amber-600" />
          <h1 className="text-xl font-semibold">Envark 遇到了界面错误 / Display error</h1>
          <p className="text-sm text-muted-foreground">
            请重新加载界面。 / Reload the interface to recover.
          </p>
          <pre className="overflow-auto whitespace-pre-wrap rounded-lg bg-muted p-3 text-xs">
            {this.state.error.message}
          </pre>
          <Button onClick={() => window.location.reload()}>重新加载 / Reload</Button>
        </div>
      </main>
    )
  }
}
