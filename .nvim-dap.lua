local status_ok, dap = pcall(require, "dap")
if not status_ok then
	print("Failed to load dap")
	return
end


dap.adapters.lldb = {
    type = "executable",
    command = "/usr/bin/lldb-vscode",
    name = "lldb",
}

dap.configurations.rust = {
    {
        name = "vulkan-test",
        type = "lldb",
        request = "launch",
        program = function()
            return vim.fn.getcwd() .. "/target/debug/vulkan-test"
        end,
        cwd = "${workspaceFolder}",
        stopOnEntry = false,
    },
}

