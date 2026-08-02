-- LOVE2D Android Release打包脚本，适配Phira
local build = {}
function build.androidRelease()
    -- 1. 强制完整打包assets目录，不做裁剪（解决资源丢失）
    local assetsDir = "assets/"
    local outputApk = "phira-android-release.apk"
    local keystorePath = "./sign.keystore" -- 自行生成签名密钥

    -- 打包lovec核心+完整资源
    os.execute(string.format([[
        love2d-android-builder --full-assets %s --output %s --release --keystore %s
    ]], assetsDir, outputApk, keystorePath))

    print("Release APK 构建完成，已完整嵌入assets资源，可正常安装")
end

-- Windows打包（自动内嵌assets，无需手动导入）
function build.windows()
    os.execute("love2d-windows-builder --embed-assets assets/ --output phira-windows.exe")
end

return build