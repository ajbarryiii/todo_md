local ok, todo = pcall(require, "todo_md")
if not ok then
  return
end

todo._bootstrap()
