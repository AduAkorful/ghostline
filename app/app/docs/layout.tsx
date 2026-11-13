import { DocsMobileNav } from "@/components/DocsMobileNav";
import { DocsNav } from "@/components/DocsNav";

// Full width, like GitBook: the sidebar sits on the left edge and the page content centres in what is left.
export default function DocsLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="bg-paper">
      <div className="flex items-start">
        <aside className="sticky top-16 hidden h-[calc(100dvh-4rem)] w-[280px] shrink-0 overflow-y-auto border-r border-rule px-3 py-6 thin-scroll lg:block">
          <DocsNav />
        </aside>
        <div className="min-w-0 flex-1">
          <DocsMobileNav />
          {children}
        </div>
      </div>
    </div>
  );
}
