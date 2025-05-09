use crate::tables::{RuneTable, RUNES, OUTPOINT_SPENDABLE_BY_ADDRESS, OUTPOINT_SPENDABLE_BY, OUTPOINT_BY_HEIGHT};
use anyhow::{anyhow, Result};
use bitcoin::OutPoint;
use crate::balance_sheet::load_sheet;
use metashrew_core::{
    flush, input, println,
    stdio::{stdout, Write},
};
use metashrew_support::index_pointer::KeyValuePointer;
use metashrew_support::utils::{consensus_decode, consensus_encode};
use protorune_support::proto::protorune::{BalanceSheet, BalanceSheetItem, Rune, OutpointResponse, ProtoruneRuneId, WalletResponse};
use protorune_support::{balance_sheet, utils::reverse_txid};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use crate::view::{protorune_outpoint_to_outpoint_response, protorunes_by_address, outpoint_to_bytes};
use serde::Serialize;
use bitcoin::hashes::Hash;
use once_cell::sync::Lazy;


pub static HTTP_URL: Lazy<String> = Lazy::new(|| {
    env!("ALKANES_HTTP_URL", "ALKANES_HTTP_URL environment variable must be set at compile time").to_string()
});

#[cfg(not(feature = "test-utils"))]
#[link(wasm_import_module = "env")]
extern "C" {
    // 声明宿主函数：接受 URL 指针、URL 长度、JSON 指针、JSON 长度
    pub fn __post_json(url_ptr: i32, url_len: i32, body_ptr: i32, body_len: i32);
}

#[no_mangle]
pub extern "C" fn post_json(url: &str, body: &str) {
    unsafe {
        __post_json(
            url.as_ptr() as i32,          // URL 字符串起始指针
            url.len() as i32,             // URL 字符串长度
            body.as_ptr() as i32,     // JSON 字符串起始指针
            body.len() as i32,        // JSON 字符串长度
        );
    }
}

pub struct BlockInfo {
    pub height: u64,
    // pub runes: Vec<(String, Rune, u128, u128, u128)>,
    pub outpoint_balances: HashMap<OutPoint, OutpointResponse>,
    // pub address_rune_balances: HashMap<String, Vec<BalanceSheetItem>>,
}

pub fn get_block_info(height: u64) -> Result<BlockInfo> {
    // 1. 获取区块部署的符文ID和原始信息
    // let mut runes = Vec::new();
    // let rune_ids = RUNES.HEIGHT_TO_RUNE_ID.select_value::<u64>(height).get_list();
    // for rune_id_bytes in rune_ids {
    //     let rune_id = balance_sheet::ProtoruneRuneId::try_from(rune_id_bytes.as_ref().to_vec())?;
    //     let etching = RUNES.RUNE_ID_TO_ETCHING.select(&rune_id.into()).get();
    //     let name = String::from_utf8_lossy(&etching).to_string();
    //     let symbol = RUNES.SYMBOL.select(&etching).get();
    //     let divisibility = RUNES.DIVISIBILITY.select(&etching).get_value::<u32>();
    //     let spacers = RUNES.SPACERS.select(&etching).get_value::<u32>();
    //     let cap = RUNES.CAP.select(&etching).get_value::<u128>();
    //     let amount = RUNES.AMOUNT.select(&etching).get_value::<u128>();
    //     let mints_remaining = RUNES.MINTS_REMAINING.select(&etching).get_value::<u128>();

    //     let rune = Rune {
    //         runeId: ::protobuf::MessageField::some(ProtoruneRuneId::default()),
    //         name,
    //         divisibility,
    //         spacers,
    //         symbol: String::from_utf8_lossy(&symbol).to_string(),
    //         special_fields: Default::default(),
    //     };

    //     runes.push((format!("{:?}:{:?}", rune_id.block, rune_id.tx), rune, cap, amount, mints_remaining));
    // }

    // 2. 使用OUTPOINT_BY_HEIGHT表直接获取该区块的所有outpoint
    let mut outpoint_balances = HashMap::new();
    let outpoints = OUTPOINT_BY_HEIGHT.select_value::<u64>(height).get_list();

    for outpoint_bytes in outpoints {
        let outpoint = consensus_decode::<OutPoint>(&mut Cursor::new(outpoint_bytes.as_ref().to_vec()))?;
        let output_len = outpoint.vout >> 16;
        let vout = outpoint.vout & 0xFFFF;
        let txid = outpoint.txid;
        let rtxid = reverse_txid(&txid);

        for i in 0..output_len {
            if i == vout {
                continue;
            }
            let _outpoint = OutPoint {
                txid,
                vout: i,
            };
            let outpoint_response = protorune_outpoint_to_outpoint_response(&_outpoint, 1).unwrap_or_else(|_| OutpointResponse::new());

            let balance_sheet = outpoint_response.balances.clone().unwrap_or_default();
            if balance_sheet.clone().entries.is_empty() {
                continue;
            }

            outpoint_balances.insert(_outpoint, outpoint_response);
        }
    }

    // 3. 获取每个地址可花费的outpoint
    // let mut address_rune_balances = HashMap::new();
    // for (outpoint, _) in &outpoint_balances {
    //     // 获取outpoint对应的地址
    //     let address = OUTPOINT_SPENDABLE_BY.select(&outpoint_to_bytes(outpoint).unwrap()).get();
    //     println!("address: {:?}, outpoint: {:?}:{:?}", hex::encode(address.as_ref()), outpoint.txid, outpoint.vout);
    //     if !address.is_empty() {

    //         let wallet_response = protorunes_by_address(&address).unwrap_or_else(|_| WalletResponse::new());

    //         for outpoint_response in wallet_response.outpoints {
    //             let balance_sheet = outpoint_response.balances.unwrap_or_default();

    //             for balance_sheet_item in balance_sheet.entries {
    //                 // let rune_id: &ProtoruneRuneId = balance_sheet_item.rune.as_ref().unwrap().runeId.as_ref().unwrap();
    //                 address_rune_balances.entry(hex::encode(address.as_ref())).or_insert(vec![]).push(balance_sheet_item);
    //             }
    //         }

    //     }
    // }

    Ok(BlockInfo {
        height,
        // runes,
        outpoint_balances,
        // address_rune_balances,
    })
}

#[derive(Serialize)]
struct SerializableBlockInfo {
    height: u64,
    // runes: Vec<SerializableRuneInfo>,
    outpoint_balances: Vec<SerializableOutpointBalance>,
    // address_rune_balances: Vec<SerializableAddressBalance>,
}

#[derive(Serialize)]
struct SerializableRuneInfo {
    id: String,
    name: String,
    divisibility: u32,
    spacers: u32,
    symbol: String,
    cap: u128,
    amount: u128,
    mints_remaining: u128,
}

#[derive(Serialize)]
struct SerializableOutpointBalance {
    txid: String,
    vout: u32,
    balances: Vec<SerializableBalanceItem>,
}

#[derive(Serialize)]
struct SerializableAddressBalance {
    address: String,
    balances: Vec<SerializableBalanceItem>,
}

#[derive(Serialize)]
struct SerializableBalanceItem {
    rune_id: String,
    balance: String,
}

impl BlockInfo {
    pub fn to_json(&self) -> serde_json::Value {
        // let runes = self.runes.iter().map(|(id, rune, cap, amount, mints_remaining)| {
        //     SerializableRuneInfo {
        //         id: id.clone(),
        //         name: rune.name.clone(),
        //         divisibility: rune.divisibility,
        //         spacers: rune.spacers,
        //         symbol: rune.symbol.clone(),
        //         cap: *cap,
        //         amount: *amount,
        //         mints_remaining: *mints_remaining,
        //     }
        // }).collect::<Vec<_>>();

        let outpoint_balances = self.outpoint_balances.iter().map(|(outpoint, outpoint_response)| {
            SerializableOutpointBalance {
                txid: format!("{:x}", outpoint.txid),
                vout: outpoint.vout,
                balances: outpoint_response.balances.clone().unwrap_or_default().entries.iter().map(|item| {
                    let balance: u128 = (u128::from(item.balance.hi) << 64) | u128::from(item.balance.lo);
                    let rune_id = item.rune.as_ref().unwrap().runeId.as_ref().unwrap();
                    let height = rune_id.height.as_ref().unwrap();
                    let txindex = rune_id.txindex.as_ref().unwrap();
                    let block: u128 = (u128::from(height.hi) << 64) | u128::from(height.lo);
                    let tx: u128 = (u128::from(txindex.hi) << 64) | u128::from(txindex.lo);
                    SerializableBalanceItem {
                        rune_id: format!("{:?}:{:?}",
                            block,
                            tx,
                        ),
                        balance: format!("{:?}", balance),
                    }
                }).collect(),
            }
        }).collect::<Vec<_>>();

        // let address_rune_balances = self.address_rune_balances.iter().map(|(address, balances)| {
        //     SerializableAddressBalance {
        //         address: address.clone(),
        //         balances: balances.iter().map(|item| {
        //             let balance: u128 = (u128::from(item.balance.hi) << 64) | u128::from(item.balance.lo);
        //             let rune_id = item.rune.as_ref().unwrap().runeId.as_ref().unwrap();
        //             let height = rune_id.height.as_ref().unwrap();
        //             let txindex = rune_id.txindex.as_ref().unwrap();
        //             let block: u128 = (u128::from(height.hi) << 64) | u128::from(height.lo);
        //             let tx: u128 = (u128::from(txindex.hi) << 64) | u128::from(txindex.lo);
        //             SerializableBalanceItem {
        //                 rune_id: format!("{:?}:{:?}",
        //                     block,
        //                     tx,
        //                 ),
        //                 balance: format!("{:?}", balance),
        //             }
        //         }).collect(),
        //     }
        // }).collect::<Vec<_>>();

        let serializable_block_info = SerializableBlockInfo {
            height: self.height,
            // runes,
            outpoint_balances,
            // address_rune_balances,
        };

        serde_json::to_value(serializable_block_info).unwrap()
    }
} 