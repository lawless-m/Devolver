# /devblog - Post to Matt's devblog

Write and publish a devblog post focused on accomplishments and how colleagues can use what we've built.

## Arguments

$ARGUMENTS - The topic or feature to write about

## Instructions

You are writing a straightforward technical devblog post. Follow this process:

### 0. Read the Blog Management Skill

First, read `~/.claude/skills/Blog Management/SKILL.md` to understand the API endpoints, authentication, and post management workflows. This skill document covers all the technical details for creating and editing blog posts.

### 1. Check Repository Context

Run `git remote get-url origin` to check the repository:
- If it contains a repo URL (GitHub, Gogs, GitLab, etc.):
  - Extract the repo name (e.g., `ReproSharepointer` from `dw.ramsden-international.com/gogs/matthew.heath/ReproSharepointer`)
  - This becomes a **project tag**: `® ReproSharepointer`
  - **IMPORTANT**: Ensure the full URL includes the correct path (e.g., `/gogs/` for Gogs repos)
  - Include the repo link in the post
- If not a git repo, discuss the work without linking or project tagging

### 2. Draft the Post

Write a post that follows these principles:

**Focus: Accomplishments and Usage**
- What was built/accomplished
- How colleagues can use it
- Practical examples or use cases
- How AI assisted (show ROI on the Claude subscription)

**Tone: Mildly enthusiastic but not overboard**
- Professional and clear
- Helpful without being preachy
- Excited about what works, honest about limitations
- Direct and practical

**Structure:**
- Hook: The problem or opportunity
- What we built: Clear explanation
- How to use it: Practical steps
- AI involvement: Be specific about what Claude helped with
- Tags: Relevant technical tags

### 3. Show the Draft

Present the draft to the user with:
```
---
[Full post content]
---

Post this? [draft / publish / tweak]
```

### 4. Publish

If approved for publishing:

1. Read the API key from `~/.claude/devblog-api-key`
2. POST to `https://dw.ramsden-international.com/devblog/api/posts` with:

```json
{
  "title": "Post title here",
  "content": "Full markdown content",
  "repo": "GitHub or Gogs URL if applicable",
  "tags": ["® RepoName", "other", "tags"],
  "publish": true
}
```

**Important:** If this repo has a remote URL, the **first tag** must be the project tag (`® RepoName`).
This automatically creates/updates the project entry on the Projects page with a snippet from this post.

Include header: `X-Cyril-Key: <api-key>`

3. Report success with the post URL: `https://dw.ramsden-international.com/devblog/#/post/<slug>`

4. Post to Teams:
   - Read the webhook URL from `~/.claude/devblog-webhook-url.txt`
   - POST an adaptive card to Teams with:

```json
{
  "type": "message",
  "attachments": [{
    "contentType": "application/vnd.microsoft.card.adaptive",
    "content": {
      "type": "AdaptiveCard",
      "version": "1.2",
      "body": [
        {
          "type": "TextBlock",
          "text": "📝 New Devblog Post",
          "weight": "bolder",
          "size": "large"
        },
        {
          "type": "TextBlock",
          "text": "Post title here",
          "size": "medium",
          "weight": "bolder",
          "wrap": true
        },
        {
          "type": "TextBlock",
          "text": "First paragraph or summary of post",
          "wrap": true,
          "maxLines": 3
        }
      ],
      "actions": [
        {
          "type": "Action.OpenUrl",
          "title": "Read Post",
          "url": "https://dw.ramsden-international.com/devblog/#/post/<slug>"
        }
      ]
    }
  }]
}
```

If saving as draft, set `"publish": false` and skip the Teams notification.

### 5. Notify Teams (Published Posts Only)

If the post was published (not draft), send a notification to Teams:

1 The Teams Webhook URL is  https://ramsdenint.webhook.office.com/webhookb2/1ae61fef-7dad-4a3b-bec0-af1209727174@c7b28d7d-7e60-48f2-b69f-6a204be54d3a/IncomingWebhook/0c8e2d3c53244e2d90449935e8305a1b/6f7e4ab9-19b8-437d-8325-61c5c636516a/V2EQRFHFQuhQyfvTxGjyd6zcXISPYOg0KMhlN2h0SP5Zg1
2. POST an Adaptive Card to the webhook URL:

```json
{
  "type": "message",
  "attachments": [
    {
      "contentType": "application/vnd.microsoft.card.adaptive",
      "content": {
        "type": "AdaptiveCard",
        "version": "1.4",
        "body": [
          {
            "type": "TextBlock",
            "text": "New Devblog Post",
            "weight": "Bolder",
            "size": "Medium"
          },
          {
            "type": "TextBlock",
            "text": "Post Title Here",
            "size": "Large",
            "weight": "Bolder",
            "wrap": true
          },
          {
            "type": "TextBlock",
            "text": "First paragraph or two of the post content as preview...",
            "wrap": true,
            "spacing": "Medium"
          },
          {
            "type": "TextBlock",
            "text": "Repository: [repo URL here]",
            "size": "Small",
            "wrap": true,
            "spacing": "Small",
            "isSubtle": true
          }
        ],
        "actions": [
          {
            "type": "Action.OpenUrl",
            "title": "Read Full Post",
            "url": "https://dw.ramsden-international.com/devblog/#/post/<slug>"
          },
          {
            "type": "Action.OpenUrl",
            "title": "View Repository",
            "url": "[repo URL here]"
          }
        ]
      }
    }
  ]
}
```

**Note:** Only include the repository TextBlock and "View Repository" action if the post has a repo URL. If there's no repo, omit both.

3. Confirm Teams notification sent successfully

### Example Topics

- "New natural language database query system deployed"
- "Automated reporting pipeline - how to use it"
- "Integration between X and Y systems"
- "Performance improvements in the data warehouse"

Always highlight how AI assisted in development or deployment.
