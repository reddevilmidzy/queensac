"use server"

interface CheckRepositoryParams {
  repositoryUrl: string
  branch: string
  fileTypes: string[]
}

interface CheckResponse {
  id: string
  status: "pending" | "processing" | "completed" | "failed"
  repository: string
  results?: {
    url: string
    status: number
    ok: boolean
    message?: string
    filePath: string
    lineNumber?: number
    lineContent?: string
    suggestedUrl?: string
  }[]
  error?: string
  prUrl?: string
}

// Explicitly set the API base URL to api.redddy.com
// This ensures we're using the correct subdomain regardless of environment variable
const API_BASE_URL = "https://api.redddy.com"

// Flag to use mock data if API is not available
const USE_MOCK_DATA = false

export async function checkRepository(params: CheckRepositoryParams): Promise<CheckResponse> {
  if (USE_MOCK_DATA) {
    return getMockCheckResponse(params.repositoryUrl)
  }

  try {
    // Format the request body according to the API specification
    const requestBody = {
      repo_url: params.repositoryUrl,
      branch: params.branch,
      // Note: fileTypes is not included as it's not in the API spec
    }

    console.log(`Sending request to ${API_BASE_URL}/check with body:`, requestBody)

    const response = await fetch(`${API_BASE_URL}/check`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
      },
      body: JSON.stringify(requestBody),
    })

    // Get the response text first to inspect it
    const responseText = await response.text()

    // Log the response for debugging
    console.log(`Response status: ${response.status}, Response text:`, responseText)

    // Try to parse the response as JSON
    let responseData
    try {
      // Only try to parse if there's actual content
      responseData = responseText ? JSON.parse(responseText) : {}
    } catch (parseError) {
      console.error("Error parsing JSON response:", parseError)
      console.error("Response text:", responseText)

      // If we can't parse the response, throw a more descriptive error
      throw new Error(
        `Invalid JSON response from API. Response status: ${response.status}. ` +
          `Response starts with: "${responseText.substring(0, 100)}${responseText.length > 100 ? "..." : ""}"`,
      )
    }

    if (!response.ok) {
      throw new Error(responseData.message || `API error: ${response.status} ${response.statusText}`)
    }

    return responseData
  } catch (error) {
    console.error("Error checking repository:", error)

    // If this is a production environment, rethrow the error
    // Otherwise, return mock data for development/testing
    if (process.env.NODE_ENV === "production") {
      throw error instanceof Error ? error : new Error("Failed to check repository. Please try again.")
    } else {
      console.log("Returning mock data due to API error")
      return getMockCheckResponse(params.repositoryUrl)
    }
  }
}

export async function cancelCheck(checkId: string): Promise<void> {
  if (USE_MOCK_DATA) {
    console.log("Mock cancel check:", checkId)
    return
  }

  try {
    // Extract repository URL and branch from checkId
    const [repositoryUrl, branch] = checkId.split("|")

    if (!repositoryUrl || !branch) {
      throw new Error("Invalid check ID format")
    }

    // Format the request body according to the API specification
    const requestBody = {
      repo_url: repositoryUrl,
      branch: branch,
    }

    console.log(`Sending DELETE request to ${API_BASE_URL}/check with body:`, requestBody)

    const response = await fetch(`${API_BASE_URL}/check`, {
      method: "DELETE",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
      },
      body: JSON.stringify(requestBody),
    })

    // Get the response text first to inspect it
    const responseText = await response.text()

    // Log the response for debugging
    console.log(`Response status: ${response.status}, Response text:`, responseText)

    if (!response.ok) {
      // Try to parse the response as JSON for error details
      let errorData = {}
      try {
        if (responseText) {
          errorData = JSON.parse(responseText)
        }
      } catch (parseError) {
        console.error("Error parsing JSON error response:", parseError)
      }

      throw new Error(errorData.message || `API error: ${response.status} ${response.statusText}`)
    }
  } catch (error) {
    console.error("Error cancelling check:", error)

    // In development, just log the error but don't throw
    if (process.env.NODE_ENV !== "production") {
      console.log("Ignoring cancel error in development mode")
      return
    }

    throw error instanceof Error ? error : new Error("Failed to cancel check. Please try again.")
  }
}

// Helper function to generate mock data for testing
function getMockCheckResponse(repositoryUrl: string): CheckResponse {
  return {
    id: "mock-id-123",
    status: "completed",
    repository: repositoryUrl,
    results: [
      {
        url: "https://example.com/working",
        status: 200,
        ok: true,
        filePath: "README.md",
        lineNumber: 10,
      },
      {
        url: "https://example.com/broken",
        status: 404,
        ok: false,
        message: "Not Found",
        filePath: "docs/guide.md",
        lineNumber: 25,
        suggestedUrl: "https://example.com/working-alternative",
      },
      {
        url: "https://github.com/broken-repo",
        status: 404,
        ok: false,
        message: "Repository not found",
        filePath: "package.json",
        lineNumber: 15,
      },
      {
        url: "https://api.example.com/v1/endpoint",
        status: 403,
        ok: false,
        message: "Forbidden",
        filePath: "src/api/client.js",
        lineNumber: 42,
        suggestedUrl: "https://api.example.com/v2/endpoint",
      },
    ],
  }
}
