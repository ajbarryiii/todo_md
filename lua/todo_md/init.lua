local M = {}

local defaults = {
  cmd = "todo_md",
  notify = true,
  open_cmd = "edit",
  keymaps = {
    enabled = true,
    open = "<leader>to",
    sync = "<leader>ts",
    where = "<leader>tw",
    repo = "<leader>tr",
  },
}

M._config = vim.deepcopy(defaults)
M._commands_registered = false

local function notify(msg, level)
  if M._config.notify then
    vim.notify(msg, level or vim.log.levels.INFO)
  end
end

local function parse_where_output(stdout)
  local out = {}
  for line in (stdout or ""):gmatch("[^\r\n]+") do
    local key, value = line:match("^([%a_]+):%s*(.+)$")
    if key and value then
      out[key] = value
    end
  end
  return out
end

local function run_cli(args, on_done)
  if not vim.system then
    on_done({ code = 1, stdout = "", stderr = "vim.system requires Neovim 0.10+" })
    return
  end

  local cmd = vim.list_extend({ M._config.cmd }, args)
  vim.system(cmd, { text = true }, function(obj)
    vim.schedule(function()
      on_done(obj)
    end)
  end)
end

local function open_path(path)
  if not path or path == "" then
    notify("todo_md: missing path", vim.log.levels.ERROR)
    return
  end
  vim.cmd(M._config.open_cmd .. " " .. vim.fn.fnameescape(path))
end

function M.where(cb)
  run_cli({ "where" }, function(obj)
    local parsed = parse_where_output(obj.stdout)
    if obj.code ~= 0 then
      local err = (obj.stderr and obj.stderr ~= "") and obj.stderr or obj.stdout
      notify("todo_md where failed: " .. (err or "unknown error"), vim.log.levels.ERROR)
      if cb then
        cb(nil, obj)
      end
      return
    end

    if cb then
      cb(parsed, obj)
    end
  end)
end

function M.open_todo()
  M.where(function(parsed)
    if not parsed or not parsed.todo then
      notify("todo_md where did not return todo path", vim.log.levels.ERROR)
      return
    end
    open_path(parsed.todo)
  end)
end

function M.open_repo()
  M.where(function(parsed)
    if not parsed or not parsed.config then
      notify("todo_md where did not return config path", vim.log.levels.ERROR)
      return
    end
    open_path(parsed.config)
  end)
end

function M.sync()
  run_cli({ "sync" }, function(obj)
    if obj.code ~= 0 then
      local err = (obj.stderr and obj.stderr ~= "") and obj.stderr or obj.stdout
      notify("todo_md sync failed: " .. (err or "unknown error"), vim.log.levels.ERROR)
      return
    end
    local msg = (obj.stdout or ""):gsub("%s+$", "")
    if msg == "" then
      msg = "todo_md sync complete"
    end
    notify(msg)
  end)
end

function M.setup_repo(remote)
  local args = { "setup" }
  if remote and remote ~= "" then
    table.insert(args, remote)
  end

  run_cli(args, function(obj)
    if obj.code ~= 0 then
      local err = (obj.stderr and obj.stderr ~= "") and obj.stderr or obj.stdout
      notify("todo_md setup failed: " .. (err or "unknown error"), vim.log.levels.ERROR)
      return
    end

    local msg = (obj.stdout or ""):gsub("%s+$", "")
    if msg == "" then
      msg = "todo_md setup complete"
    end
    notify(msg)
  end)
end

function M.show_where()
  M.where(function(parsed, obj)
    if not parsed then
      return
    end

    local lines = {
      "todo_md where",
      "config: " .. (parsed.config or "<missing>"),
      "todo: " .. (parsed.todo or "<missing>"),
      "env: " .. (parsed.env or "<missing>"),
      "branch: " .. (parsed.branch or "<missing>"),
      "remote: " .. (parsed.remote or "<missing>"),
      "",
      "raw:",
      (obj.stdout or ""):gsub("%s+$", ""),
    }

    vim.cmd("new")
    local buf = vim.api.nvim_get_current_buf()
    vim.bo[buf].buftype = "nofile"
    vim.bo[buf].bufhidden = "wipe"
    vim.bo[buf].swapfile = false
    vim.bo[buf].filetype = "markdown"
    vim.api.nvim_buf_set_name(buf, "todo_md://where")
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
    vim.bo[buf].modifiable = false
  end)
end

function M._register_commands()
  if M._commands_registered then
    return
  end

  vim.api.nvim_create_user_command("TodoOpen", function()
    M.open_todo()
  end, { desc = "Open todo.md" })

  vim.api.nvim_create_user_command("TodoSync", function()
    M.sync()
  end, { desc = "Sync todo.md with git remote" })

  vim.api.nvim_create_user_command("TodoWhere", function()
    M.show_where()
  end, { desc = "Show todo_md resolved paths/config" })

  vim.api.nvim_create_user_command("TodoRepo", function()
    M.open_repo()
  end, { desc = "Open todos config git repo directory" })

  vim.api.nvim_create_user_command("TodoSetup", function(opts)
    local remote = opts.fargs[1]
    M.setup_repo(remote)
  end, {
    nargs = "?",
    complete = "file",
    desc = "Initialize todos config directory and git remote",
  })

  M._commands_registered = true
end

function M._register_keymaps()
  local km = M._config.keymaps
  if not km or km.enabled == false then
    return
  end

  local set = vim.keymap.set
  if km.open and km.open ~= "" then
    set("n", km.open, "<cmd>TodoOpen<cr>", { desc = "Todo: open todo.md" })
  end
  if km.sync and km.sync ~= "" then
    set("n", km.sync, "<cmd>TodoSync<cr>", { desc = "Todo: sync" })
  end
  if km.where and km.where ~= "" then
    set("n", km.where, "<cmd>TodoWhere<cr>", { desc = "Todo: where" })
  end
  if km.repo and km.repo ~= "" then
    set("n", km.repo, "<cmd>TodoRepo<cr>", { desc = "Todo: open repo" })
  end
end

function M.setup(opts)
  M._config = vim.tbl_deep_extend("force", M._config, opts or {})
  M._register_commands()
  M._register_keymaps()
end

function M._bootstrap()
  M._register_commands()
  M._register_keymaps()
end

return M
