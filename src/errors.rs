/// global errors
#[derive(Copy, PartialEq, Eq, Clone, Debug)]
pub enum Error {
    //无效的账户
    InvalidAccount,
    //无效的公钥
    InvalidPublicKey,
    //无效的私钥
    InvalidPrivateKey,
    //无效的签名
    InvalidSignature,
    //无效的参数
    InvalidParam,
    //签名错误
    SignatureErr,
    //验签错误
    VerifySignErr,
    //脚本执行错误
    ScriptExeErr,
    //脚本格式错误
    ScriptFmtErr,
    //空脚本
    ScriptEmptyErr,
    //堆栈长度错误
    StackLenErr,
    //堆栈溢出
    StackOverlowErr,
}
