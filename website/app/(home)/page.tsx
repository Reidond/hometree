import { 
  ArrowRight, 
  BoxSelect, 
  FolderCheck, 
  GitBranch, 
  Github,
  Lock, 
  RotateCcw, 
  Terminal,
  Zap
} from 'lucide-react';
import Link from 'next/link';

export default function HomePage() {
  return (
    <main className="flex flex-col min-h-screen">
      <section className="relative px-6 pt-32 pb-24 text-center md:pt-48 md:pb-32 overflow-hidden">
        <div className="absolute inset-0 -z-10 bg-[radial-gradient(ellipse_at_top,var(--tw-gradient-stops))] from-blue-100/20 via-transparent to-transparent dark:from-blue-900/20 dark:via-transparent dark:to-transparent" />
        
        <div className="max-w-4xl mx-auto space-y-8">
          <div className="inline-flex items-center px-3 py-1 rounded-full border border-neutral-200 dark:border-neutral-800 bg-neutral-50 dark:bg-neutral-900 text-sm text-neutral-600 dark:text-neutral-400 mb-4">
            <span className="flex h-2 w-2 rounded-full bg-blue-500 mr-2 animate-pulse"></span>
            v0.1.0 Now Available
          </div>
          
          <h1 className="text-5xl md:text-7xl font-bold tracking-tight text-neutral-900 dark:text-white">
            Manage your dotfiles <br className="hidden md:block" />
            <span className="text-transparent bg-clip-text bg-linear-to-br from-blue-600 to-violet-600 dark:from-blue-400 dark:to-violet-400">
              with confidence
            </span>
          </h1>
          
          <p className="text-xl md:text-2xl text-neutral-600 dark:text-neutral-300 max-w-2xl mx-auto leading-relaxed">
            Linux-first CLI for managing your dotfiles with git.
            Secure, selective, and built for modern workflows.
          </p>
          
          <div className="flex flex-col sm:flex-row items-center justify-center gap-4 pt-4">
            <Link 
              href="/docs" 
              className="px-8 py-3.5 rounded-full bg-neutral-900 dark:bg-white text-white dark:text-neutral-900 font-semibold hover:opacity-90 transition-opacity flex items-center gap-2"
            >
              Get Started <ArrowRight className="w-4 h-4" />
            </Link>
            <a 
              href="https://github.com/Reidond/hometree" 
              target="_blank"
              rel="noreferrer"
              className="px-8 py-3.5 rounded-full border border-neutral-200 dark:border-neutral-800 hover:bg-neutral-50 dark:hover:bg-neutral-900 transition-colors flex items-center gap-2 text-neutral-700 dark:text-neutral-300 font-medium"
            >
              <Github className="w-5 h-5" /> GitHub
            </a>
          </div>
        </div>
      </section>

      <section className="px-6 py-24 bg-neutral-50 dark:bg-neutral-900/50">
        <div className="max-w-6xl mx-auto">
          <div className="text-center mb-16">
            <h2 className="text-3xl font-bold text-neutral-900 dark:text-white mb-4">
              Everything you need
            </h2>
            <p className="text-neutral-600 dark:text-neutral-400 max-w-2xl mx-auto">
              Built to solve the pain points of existing dotfile managers using modern tooling.
            </p>
          </div>

          <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-8">
            <FeatureCard 
              icon={<BoxSelect />}
              title="Selective Tracking"
              description="Track only what you choose. No full-home git repositories or accidental commits of sensitive files."
            />
            <FeatureCard 
              icon={<GitBranch />}
              title="Git-Powered"
              description="Full version history with snapshots. Branch, merge, and rebase your configuration just like code."
            />
            <FeatureCard 
              icon={<Lock />}
              title="Encrypted Secrets"
              description="Age-encrypted sidecars ensure your secrets are safe. Plaintext never touches your git history."
            />
            <FeatureCard 
              icon={<Zap />}
              title="Auto-Staging Daemon"
              description="Event-driven watcher stages changes automatically. Efficient and fast without recursive scans."
            />
            <FeatureCard 
              icon={<RotateCcw />}
              title="Safe Rollbacks"
              description="Deploy and rollback with confidence. Restore previous configurations instantly if something breaks."
            />
            <FeatureCard 
              icon={<FolderCheck />}
              title="XDG-Compliant"
              description="Respects standard Linux paths and conventions. Keeps your home directory clean and organized."
            />
          </div>
        </div>
      </section>

      <section className="px-6 py-24">
        <div className="max-w-5xl mx-auto">
          <div className="flex flex-col lg:flex-row items-center gap-12">
            <div className="flex-1 space-y-6">
              <h2 className="text-3xl font-bold text-neutral-900 dark:text-white">
                Up and running in seconds
              </h2>
              <p className="text-lg text-neutral-600 dark:text-neutral-400 leading-relaxed">
                Install the CLI, initialize your repository, and start tracking your first config file. It's that simple.
              </p>
              <div className="flex flex-col gap-4">
                <div className="flex items-start gap-3">
                  <div className="mt-1 bg-blue-100 dark:bg-blue-900/30 p-1.5 rounded-md text-blue-600 dark:text-blue-400">
                    <Terminal className="w-5 h-5" />
                  </div>
                  <div>
                    <h3 className="font-semibold text-neutral-900 dark:text-white">Simple CLI</h3>
                    <p className="text-neutral-600 dark:text-neutral-400 text-sm">Intuitive commands that feel familiar to git users.</p>
                  </div>
                </div>
                <div className="flex items-start gap-3">
                  <div className="mt-1 bg-purple-100 dark:bg-purple-900/30 p-1.5 rounded-md text-purple-600 dark:text-purple-400">
                    <Lock className="w-5 h-5" />
                  </div>
                  <div>
                    <h3 className="font-semibold text-neutral-900 dark:text-white">Secure by Default</h3>
                    <p className="text-neutral-600 dark:text-neutral-400 text-sm">Your secrets are encrypted before they hit the disk.</p>
                  </div>
                </div>
              </div>
            </div>

            <div className="flex-1 w-full max-w-xl">
              <div className="rounded-xl overflow-hidden bg-neutral-900 shadow-2xl border border-neutral-800">
                <div className="flex items-center gap-2 px-4 py-3 border-b border-neutral-800 bg-neutral-900/50">
                  <div className="w-3 h-3 rounded-full bg-red-500/20 border border-red-500/50" />
                  <div className="w-3 h-3 rounded-full bg-yellow-500/20 border border-yellow-500/50" />
                  <div className="w-3 h-3 rounded-full bg-green-500/20 border border-green-500/50" />
                </div>
                <div className="p-6 font-mono text-sm overflow-x-auto">
                  <div className="flex gap-2">
                    <span className="text-purple-400">$</span>
                    <span className="text-neutral-300">cargo install --path crates/hometree-cli</span>
                  </div>
                  <div className="flex gap-2 mt-2">
                    <span className="text-purple-400">$</span>
                    <span className="text-neutral-300">hometree init</span>
                  </div>
                  <div className="text-emerald-500/70 italic my-1">Initialized empty hometree repository in ~/.local/share/hometree</div>
                  <div className="flex gap-2 mt-2">
                    <span className="text-purple-400">$</span>
                    <span className="text-neutral-300">hometree track ~/.config/myapp/config.toml</span>
                  </div>
                  <div className="flex gap-2 mt-2">
                    <span className="text-purple-400">$</span>
                    <span className="text-neutral-300">hometree snapshot -m "track config"</span>
                  </div>
                  <div className="text-emerald-500/70 italic my-1">Created snapshot 8a2b9f1: track config</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section className="py-24 border-t border-neutral-200 dark:border-neutral-800 bg-neutral-50/50 dark:bg-neutral-900/20">
        <div className="max-w-4xl mx-auto text-center px-6">
          <h2 className="text-3xl font-bold text-neutral-900 dark:text-white mb-6">
            Ready to take control?
          </h2>
          <p className="text-lg text-neutral-600 dark:text-neutral-400 mb-8 max-w-xl mx-auto">
            Join the developers who are bringing sanity to their dotfiles management.
          </p>
          <Link 
            href="/docs" 
            className="inline-flex items-center px-8 py-3.5 rounded-full bg-neutral-900 dark:bg-white text-white dark:text-neutral-900 font-semibold hover:opacity-90 transition-opacity"
          >
            Read the Documentation
          </Link>
        </div>
      </section>
    </main>
  );
}

function FeatureCard({ icon, title, description }: { icon: React.ReactNode, title: string, description: string }) {
  return (
    <div className="group p-6 rounded-2xl bg-white dark:bg-neutral-800/50 border border-neutral-200 dark:border-neutral-800 hover:border-blue-500/30 dark:hover:border-blue-500/30 transition-all duration-300 hover:shadow-lg hover:shadow-blue-500/5">
      <div className="mb-4 inline-flex p-3 rounded-xl bg-neutral-100 dark:bg-neutral-900 text-blue-600 dark:text-blue-400 group-hover:scale-110 transition-transform duration-300">
        {icon}
      </div>
      <h3 className="text-xl font-semibold mb-2 text-neutral-900 dark:text-white">
        {title}
      </h3>
      <p className="text-neutral-600 dark:text-neutral-400 leading-relaxed">
        {description}
      </p>
    </div>
  );
}
