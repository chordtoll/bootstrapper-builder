use int_enum::IntEnum;

#[repr(u8)]
#[derive(Debug, PartialEq, IntEnum)]
pub enum ClientRequest {
    AddRecipe = 0,
    GetStatus = 1,
    GetHash = 2,
    Goodbye = 3,
}
