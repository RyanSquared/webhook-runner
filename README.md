![example workflow](https://github.com/RyanSquared/webhook-runner/actions/workflows/ci.yaml/badge.svg)

# webhook-runner
rust program to run thing once webhook is hit

# Plans:

I would like a system that can determine which commands to run based off the
result from the JSON blob as well as a way to format that data (such as a
commit ID, a tag name, etc.) into the command. This isn't a hard requirement as
the commands can either shell out to JQ or can be written in a language with
structured data that can just understand JSON. I think by default the JSON
should be passed into the stdin of the launched command.

At the current point, the HTTP request stays open until the request has
completed. I think perhaps we should set it up to spawn the task in the
background and just let it run. I don't think we need to wait on it. Especially
for things that can take a significant amount of time to process, such as
running a Terraform deployment, or building a Docker container from scratch.
