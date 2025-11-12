# API Test Server
    一个轻量级的模拟 API 服务器，用于在服务端开发完成前测试客户端接口功能。

## 🎯 项目简介
    在服务端开发尚未完成但已知接口数据结构的情况下，API Test Server 可以帮助您快速搭建模拟服务，方便客户端进行接口测试和功能验证。

## 主要特性
    🚀 快速部署 - 单文件执行，无需复杂配置
    📡 模拟真实接口 - 完全模拟服务端 API 响应
    🔧 灵活配置 - 支持自定义响应数据和接口参数
    📱 客户端友好 - 移动端、Web 端均可直接调用
## 🛠️ 使用方法
### 快速开始
    1. 配置响应文件
        在可执行文件同级目录下创建 [route_response.json](route_response.json) 文件，根据您的项目需求修改响应内容：
```
[{
    "url": "/api/user",
    "method": "get",
    "headers": {
      "content-type": "application/json"
    },
    "response": {
      "url": "http://localhost:8080/firmware/2.0.0.bin",
      "version": "2.0.0",
      "md5": "abcd1234567890abcd1234567890abcd",
      "instructions": [
        {
          "type": "reboot",
          "requestId": "550e8400-e29b-41d4-a716-446655440000",
          "additionalParameters": {
            "rebootTime": "AfterFirmwareDownload"
          }
        }
      ]
    }
  }]
```
    2. 启动服务
        shell:
``` 
        ./api_test_server
```
    3. 测试接口
    客户端现在可以访问配置的接口地址，例如：http://localhost:8080/api/user

## 详细配置
    根据客户端接口需求，您可以配置：
    *不同的 HTTP 方法（GET、POST、PUT、DELETE）
    *自定义 HTTP 状态码
    *响应头信息
    *延迟响应时间
    *动态响应数据

## 📋 配置示例
    参考项目中的 route_response.json 文件示例，了解完整的配置选项和语法。

## 🔄 工作流程
    客户端请求 → API Test Server → 读取配置 → 返回预设响应
## 💡 使用场景
    🔄 前后端并行开发 - 后端开发期间前端可独立测试
    📲 移动端测试 - 在真实服务部署前测试移动应用
    🧪 接口自动化测试 - 作为测试环境的稳定数据源
    🎭 多种场景模拟 - 模拟成功、失败、异常等不同响应状态

## ⚡ 注意事项
    确保 route_response.json 文件格式正确
    根据实际需求调整接口路径和响应数据
    生产环境使用前请进行充分测试