"use client"

import { useState } from "react"
import { AlertCircle, Check, ExternalLink, FileText, Github, Loader2, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { toast } from "@/components/ui/use-toast"
import { Checkbox } from "@/components/ui/checkbox"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { checkRepository, cancelCheck } from "@/lib/api-client"

interface LinkResult {
  url: string
  status: number
  ok: boolean
  message?: string
  filePath: string
  lineNumber?: number
  lineContent?: string
  suggestedUrl?: string
}

interface CheckResponse {
  id: string
  status: "pending" | "processing" | "completed" | "failed"
  repository: string
  results?: LinkResult[]
  error?: string
  prUrl?: string
}

export default function QueensacApp() {
  const [repoUrl, setRepoUrl] = useState("")
  const [branch, setBranch] = useState("main")
  const [includeMarkdown, setIncludeMarkdown] = useState(true)
  const [includeHtml, setIncludeHtml] = useState(true)
  const [includeJs, setIncludeJs] = useState(true)
  const [includeTxt, setIncludeTxt] = useState(true)
  const [loading, setLoading] = useState(false)
  const [checkId, setCheckId] = useState<string | null>(null)
  const [checkResponse, setCheckResponse] = useState<CheckResponse | null>(null)
  const [activeTab, setActiveTab] = useState("all")

  const handleCheck = async () => {
    if (!repoUrl) {
      toast({
        title: "Error",
        description: "Please enter a GitHub repository URL",
        variant: "destructive",
      })
      return
    }

    // Validate GitHub URL format
    const githubUrlPattern = /^https?:\/\/github\.com\/([^/]+)\/([^/]+)/i
    const match = repoUrl.match(githubUrlPattern)

    if (!match) {
      toast({
        title: "Error",
        description: "Please enter a valid GitHub repository URL (e.g., https://github.com/owner/repo)",
        variant: "destructive",
      })
      return
    }

    setLoading(true)
    setCheckResponse(null)

    try {
      const fileTypes = []
      if (includeMarkdown) fileTypes.push(".md")
      if (includeHtml) fileTypes.push(".html", ".htm")
      if (includeJs) fileTypes.push(".js", ".jsx", ".ts", ".tsx")
      if (includeTxt) fileTypes.push(".txt")

      const response = await checkRepository({
        repositoryUrl: repoUrl,
        branch,
        fileTypes, // This will be ignored by the API client but kept for future compatibility
      })

      // Store the repository URL and branch as the checkId for cancellation
      setCheckId(`${repoUrl}|${branch}`)
      setCheckResponse(response)

      if (response.status === "failed") {
        toast({
          title: "Check Failed",
          description: response.error || "Failed to check repository",
          variant: "destructive",
        })
      } else if (response.status === "completed") {
        toast({
          title: "Check Completed",
          description: `Found ${response.results?.length || 0} links in the repository`,
        })
      } else {
        // For pending or processing status
        toast({
          title: "Check Initiated",
          description: "The repository check has been started",
        })
      }
    } catch (error) {
      toast({
        title: "Error",
        description: error instanceof Error ? error.message : "Failed to check repository",
        variant: "destructive",
      })
    } finally {
      setLoading(false)
    }
  }

  const handleCancel = async () => {
    if (!checkId) return

    try {
      await cancelCheck(checkId)
      toast({
        title: "Check Cancelled",
        description: "The repository check has been cancelled",
      })
      setCheckId(null)
      setLoading(false)
    } catch (error) {
      toast({
        title: "Error",
        description: error instanceof Error ? error.message : "Failed to cancel check",
        variant: "destructive",
      })
    }
  }

  const results = checkResponse?.results || []
  const brokenLinks = results.filter((r) => !r.ok)
  const fixableLinks = brokenLinks.filter((r) => r.suggestedUrl)
  const unfixableLinks = brokenLinks.filter((r) => !r.suggestedUrl)

  const filteredResults =
    activeTab === "all"
      ? results
      : activeTab === "broken"
        ? brokenLinks
        : activeTab === "fixable"
          ? fixableLinks
          : unfixableLinks

  const groupedByFile = filteredResults.reduce(
    (acc, result) => {
      if (!acc[result.filePath]) {
        acc[result.filePath] = []
      }
      acc[result.filePath].push(result)
      return acc
    },
    {} as Record<string, LinkResult[]>,
  )

  return (
    <div className="container mx-auto py-10">
      <div className="flex flex-col items-center mb-8">
        <Github className="h-12 w-12 mb-2" />
        <h1 className="text-3xl font-bold text-center">queensac</h1>
        <p className="text-muted-foreground text-center max-w-2xl mt-2">
          Scan a GitHub repository for broken links and automatically create a PR with fixes.
        </p>
      </div>

      <div className="grid grid-cols-1 gap-8">
        <Card>
          <CardHeader>
            <CardTitle>Repository Details</CardTitle>
            <CardDescription>Enter the GitHub repository URL you want to scan for broken links.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="repo-url">GitHub Repository URL</Label>
              <Input
                id="repo-url"
                placeholder="https://github.com/owner/repo"
                value={repoUrl}
                onChange={(e) => setRepoUrl(e.target.value)}
                disabled={loading}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="branch">Branch</Label>
              <Input
                id="branch"
                placeholder="main"
                value={branch}
                onChange={(e) => setBranch(e.target.value)}
                disabled={loading}
              />
            </div>
            <div className="space-y-2">
              <Label>File Types to Scan</Label>
              <div className="flex flex-wrap gap-4 mt-2">
                <div className="flex items-center space-x-2">
                  <Checkbox
                    id="markdown"
                    checked={includeMarkdown}
                    onCheckedChange={(checked) => setIncludeMarkdown(checked === true)}
                    disabled={loading}
                  />
                  <Label htmlFor="markdown">Markdown (.md)</Label>
                </div>
                <div className="flex items-center space-x-2">
                  <Checkbox
                    id="html"
                    checked={includeHtml}
                    onCheckedChange={(checked) => setIncludeHtml(checked === true)}
                    disabled={loading}
                  />
                  <Label htmlFor="html">HTML (.html, .htm)</Label>
                </div>
                <div className="flex items-center space-x-2">
                  <Checkbox
                    id="js"
                    checked={includeJs}
                    onCheckedChange={(checked) => setIncludeJs(checked === true)}
                    disabled={loading}
                  />
                  <Label htmlFor="js">JavaScript/TypeScript (.js, .jsx, .ts, .tsx)</Label>
                </div>
                <div className="flex items-center space-x-2">
                  <Checkbox
                    id="txt"
                    checked={includeTxt}
                    onCheckedChange={(checked) => setIncludeTxt(checked === true)}
                    disabled={loading}
                  />
                  <Label htmlFor="txt">Text (.txt)</Label>
                </div>
              </div>
            </div>
          </CardContent>
          <CardFooter className="flex justify-between">
            {loading && checkId ? (
              <Button variant="destructive" onClick={handleCancel}>
                Cancel Check
              </Button>
            ) : (
              <Button variant="outline" disabled>
                Cancel Check
              </Button>
            )}
            <Button onClick={handleCheck} disabled={loading}>
              {loading ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Checking Repository...
                </>
              ) : (
                "Check Repository"
              )}
            </Button>
          </CardFooter>
        </Card>

        {checkResponse && (
          <Alert variant={checkResponse.status === "failed" ? "destructive" : "default"}>
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>
              Status: {checkResponse.status.charAt(0).toUpperCase() + checkResponse.status.slice(1)}
            </AlertTitle>
            <AlertDescription>
              {checkResponse.status === "failed"
                ? checkResponse.error
                : checkResponse.status === "completed"
                  ? `Found ${checkResponse.results?.length || 0} links (${checkResponse.results?.filter((r) => r.ok).length || 0} valid, ${checkResponse.results?.filter((r) => !r.ok).length || 0} broken)`
                  : "The repository check is in progress. Results will be displayed when available."}
            </AlertDescription>
          </Alert>
        )}

        {checkResponse?.prUrl && (
          <Alert>
            <Check className="h-4 w-4" />
            <AlertTitle>Pull Request Created</AlertTitle>
            <AlertDescription className="flex items-center">
              <span>A pull request has been created to fix broken links: </span>
              <a
                href={checkResponse.prUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="ml-2 text-blue-600 hover:underline flex items-center"
              >
                View PR <ExternalLink className="h-3 w-3 ml-1" />
              </a>
            </AlertDescription>
          </Alert>
        )}

        {checkResponse?.results && checkResponse.results.length > 0 && (
          <Card className="h-full">
            <CardHeader>
              <CardTitle>Results</CardTitle>
              <CardDescription>
                Found {results.length} links ({results.filter((r) => r.ok).length} valid, {brokenLinks.length} broken)
              </CardDescription>
            </CardHeader>
            <CardContent className="p-0">
              <Tabs defaultValue="all" value={activeTab} onValueChange={setActiveTab} className="w-full">
                <div className="px-6 pt-2">
                  <TabsList className="grid w-full grid-cols-4">
                    <TabsTrigger value="all">All Links ({results.length})</TabsTrigger>
                    <TabsTrigger value="broken">All Broken ({brokenLinks.length})</TabsTrigger>
                    <TabsTrigger value="fixable">Fixable ({fixableLinks.length})</TabsTrigger>
                    <TabsTrigger value="unfixable">Unfixable ({unfixableLinks.length})</TabsTrigger>
                  </TabsList>
                </div>

                <TabsContent value={activeTab} className="m-0">
                  <div className="max-h-[600px] overflow-y-auto p-6 space-y-6">
                    {Object.entries(groupedByFile).map(([filePath, fileResults]) => (
                      <div key={filePath} className="space-y-2">
                        <div className="flex items-center space-x-2">
                          <FileText className="h-4 w-4 text-muted-foreground" />
                          <h3 className="text-sm font-medium">{filePath}</h3>
                          <Badge variant="outline">{fileResults.length} links</Badge>
                        </div>
                        <div className="space-y-2 pl-6">
                          {fileResults.map((result, index) => (
                            <div
                              key={`${filePath}-${index}`}
                              className={`p-3 rounded-md ${
                                result.ok
                                  ? "bg-green-50 border border-green-100"
                                  : result.suggestedUrl
                                    ? "bg-yellow-50 border border-yellow-100"
                                    : "bg-red-50 border border-red-100"
                              }`}
                            >
                              <div className="flex items-start justify-between">
                                <div className="flex-1 mr-2 break-all">
                                  <div className="flex items-center">
                                    {result.ok ? (
                                      <Check className="h-4 w-4 text-green-500 mr-2 shrink-0" />
                                    ) : result.suggestedUrl ? (
                                      <Badge className="mr-2 bg-yellow-500 hover:bg-yellow-500">Fixable</Badge>
                                    ) : (
                                      <X className="h-4 w-4 text-red-500 mr-2 shrink-0" />
                                    )}
                                    <a
                                      href={result.url}
                                      target="_blank"
                                      rel="noopener noreferrer"
                                      className="text-sm font-medium hover:underline flex items-center"
                                    >
                                      {result.url.length > 50 ? `${result.url.substring(0, 50)}...` : result.url}
                                      <ExternalLink className="h-3 w-3 ml-1" />
                                    </a>
                                  </div>
                                  {result.lineNumber && (
                                    <p className="text-xs text-muted-foreground ml-6">Line: {result.lineNumber}</p>
                                  )}
                                  {!result.ok && result.message && (
                                    <p
                                      className={`text-xs mt-1 ml-6 ${
                                        result.suggestedUrl ? "text-yellow-600" : "text-red-600"
                                      }`}
                                    >
                                      {result.message}
                                    </p>
                                  )}
                                  {result.suggestedUrl && (
                                    <div className="ml-6 mt-2 text-xs">
                                      <span className="text-muted-foreground">Suggested URL: </span>
                                      <a
                                        href={result.suggestedUrl}
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        className="text-blue-600 hover:underline"
                                      >
                                        {result.suggestedUrl}
                                      </a>
                                    </div>
                                  )}
                                </div>
                                <span
                                  className={`text-xs font-mono px-2 py-1 rounded ${
                                    result.ok
                                      ? "bg-green-200 text-green-800"
                                      : result.suggestedUrl
                                        ? "bg-yellow-200 text-yellow-800"
                                        : "bg-red-200 text-red-800"
                                  }`}
                                >
                                  {result.status}
                                </span>
                              </div>
                            </div>
                          ))}
                        </div>
                      </div>
                    ))}

                    {Object.keys(groupedByFile).length === 0 && (
                      <div className="flex items-center justify-center p-8 text-muted-foreground">
                        No links found in this category.
                      </div>
                    )}
                  </div>
                </TabsContent>
              </Tabs>
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  )
}
