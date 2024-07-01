// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use std::str::FromStr;
use std::sync::Arc;

use alloy_primitives::FixedBytes;
use alloy_sol_macro::sol;
use alloy_sol_types::{SolCall, SolInterface};
use cargo_stylus_util::sys;
use clap::{Args, Parser};
use ethers::middleware::{Middleware, SignerMiddleware};
use ethers::providers::{Http, Provider, ProviderError, RawCall};
use ethers::signers::{LocalWallet, Signer};
use ethers::types::spoof::State;
use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::{Address, Eip1559TransactionRequest, NameOrAddress, H160, U256};
use ethers::utils::keccak256;
use eyre::{bail, eyre, Context, ErrReport, Result};
use hex::FromHex;
use serde_json::Value;

#[derive(Parser, Clone, Debug)]
#[command(name = "cargo-stylus-cache")]
#[command(bin_name = "cargo stylus cache")]
#[command(author = "Offchain Labs, Inc.")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Cargo command for interacting with the Arbitrum Stylus cache manager", long_about = None)]
#[command(propagate_version = true)]
pub struct Opts {
    #[command(subcommand)]
    command: Subcommands,
}

#[derive(Parser, Debug, Clone)]
enum Subcommands {
    #[command(alias = "p")]
    Program(CacheArgs),
}

#[derive(Args, Clone, Debug)]
struct CacheArgs {
    /// RPC endpoint.
    #[arg(short, long, default_value = "https://sepolia-rollup.arbitrum.io/rpc")]
    endpoint: String,
    /// Address of the Stylus program to cache.
    #[arg(short, long)]
    address: Address,
    /// Bid, in wei, to place on the program cache.
    #[arg(short, long, hide(true))]
    bid: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Opts::parse();
    macro_rules! run {
        ($expr:expr, $($msg:expr),+) => {
            $expr.await.wrap_err_with(|| eyre!($($msg),+))
        };
    }
    match args.command {
        Subcommands::Program(args) => run!(
            self::cache_program(args),
            "failed to submit program cache request"
        ),
    }
}

sol! {
    interface CacheManager {
        function placeBid(bytes32 codehash) external payable;

        error NotChainOwner(address sender);
        error AsmTooLarge(uint256 asm, uint256 queueSize, uint256 cacheSize);
        error AlreadyCached(bytes32 codehash);
        error BidTooSmall(uint192 bid, uint192 min);
        error BidsArePaused();
        error MakeSpaceTooLarge(uint64 size, uint64 limit);
    }
}

async fn cache_program(args: CacheArgs) -> Result<()> {
    let provider: Provider<Http> = sys::new_provider(&args.endpoint)?;
    let chain_id = provider.get_chainid().await?;
    println!("Connected to chain {}", chain_id);

    let program_code = provider
        .get_code(args.address, None)
        .await
        .wrap_err("failed to fetch program code")?;
    println!("Program code: {:?}", hex::encode(&program_code));
    // let codehash = ethers::utils::keccak256(&program_code);
    // println!("Program codehash: {:#x}", &codehash);

    let raw_data = hex::decode("eff000001b242b23113662924a1045d5e667807a496e0e0117a87f49c7881338b04573d0e85a3033bcdcea73ffb8e29e93faa31153586c73d8f45c9b44c5d23c429259ffabd4f67ff7c86aab35a8c95a646a5f0f49269fe425cf8772da904e7b5b7173b548327220ce6c0e37c8d77c839b939618223409d01d04926f375575284b93b4dd9e2fb0fb101f11c543f92d811af817f6db3f498887b27b12593431444b18fa5d503686c138a4bf1971f630ce4f75a9aeb4b548e3dea02ff2a1ae2ae2900bd46c7d93793f826a35f9de2412d9b30a89441ea790482492422191f9268f432291482432f1bf54d3f6eddf5d920a691d422e7a123773b553eda616174b815800940f202f00941c4825523cc504f2648b9243084de92ae4d8b9693dbd0d4ffbc9ea67bc97b69aae2dc67d766ce18a8af461c86235ca6faa1bc34d7f8908f3f38f1ec3d5f5eea33615a404dcf717a52b3f9f5322e253b609ce9a95515359e6969729909a7076fd969ca97bb8c74c3ec7f3f5f5f1b3f2f9bc8ddff5a7e4af7ecf2454690b34ee1bf78fdbcf842bc2bad5bd67bcafe7f3f1b17390a8b7a825a25de3f1f4fd7917a2ce830765998741ffc3120ddabeeef8ff6fa285a10d1b46f3ba096b57513a8a0a05bdf09eaf54d04cdc7382ce58f259b219c98e23ab6c7fb778c5f6e04833ea959e0d71a689a98838088db962eb25a5d2fa9bdcaafae092b35dff97adc76fa1840fca5569c7dabbafa8a24cdf7ae53c8464b6d0624edd3e1823244fb0db774532f70207516e8fbddbfc840b794361a4dae013ac52a6b16ce7d11868cbbfe6fdb5afa241a3d30ca65f04f583b60c88cac830230c39e684a32aab8a2a814180eaac2eea743e3b5f9c4f4bd812b18452968a9446d88818a1d56cb5584debd83ab18e9aac299a34c6c6c4185dc7ae13d7d1349b16d371ced7a99c479cf37597ba27d86d93f99b4bb67340d8615879de8db610e59b2d038285dd81c63b9e545696c8ca450d2c06d648f6810a6bb5ea77c6c8c351b039078f16123db6a1d6c3cec2174298163b3e52909cc7277b436efc817a9e0a4a474d6fea25b828a6b2bce00fd4960ffc8386f263315bd3c252b6e662da9062aea603a9e666ba9066eea607e9e661fa90619e5ec42ec8342f935896799bd4b2cd87649643be406eb9c037282c37f801a5f549ab42ab7869181a19b136fdcab2e90f0176fd09e4b6fd00aa009f2d7378cce076278ccc16c2b0fc303b6bf89cbf4a2359b42a66a33e8892d08788ee3caa40bf66d9f38a935f484f8f6c1cd59a4d85adfa2381ecfa19e4a47b764faa624fcdc2afdf4141fd0714a63b8e4e6ac9b17a89a47e03a5f55750966e393fa905176a1065fd0aaad4896bb5e146cdd57e6023ba76458110915892cb4715a8d55f955aa3d5157f35cc2214190f1a9bc38507bf764ec212ddea2d8348a33398ac6a9fedc34016e06da95f7624bdc9d24353995ca16c683d34f52407dffb0fff597e105205da21e333344c5e0fb3899ad836331930c2124c00fbf5cf48015bc6ebe699a41088a3051b727858b70fb15aa29530065b036be102d4f7e6d7860ad9e68a1447b0fce30b28a1da13a49761d01fe06cbe08b80dadbdab6f77b8ca992818c207727f560a1e7c1d1167987c3171c87041f89ea181beb51413fdd99800bd7931cc2812b8c0dfdc024e09ca8f13125d1644ffd16f01e0810f88be11e180bc2c21c8ec519ff77525d9f44dd5d5ec7ffeda386c6a6487977536ec3bcccf22432f27d4aa3c54848569a0b1bf430a866da779a92fd275211c272a5cd79dd9c5488ce9ba23072a17eec399231b165ad15d775166428fe1babd2a370e9c5f6ce086ba32875e531a53f42fba36f7a8fd1330f89b8e96cd91da5b032cf6f58cdcdf1285bf204a36c068ef6784f6e70cf67744ec2f8984bd00ede30cb0e733667f4f787e45f86c06cdfe9d21ede50cc33f12a6bf260483eecd7c8a6cc512d912b91deec47dad8bd3963dec6d3decb234a074370b95549a9c031ded9458d43028cadb892faceb5961aff0af970f89090cc606922d27f864fb913094e176411e94c15888db50e063a6c201a01dea5bbb2c256f4149458dc608a70e8749104c82666f0ec90714bc1a386875afd70f1a501d1cfbc0557e61d80f4ce13781ded0cc2467ac53be5c9304573130f28f5e6de58c3d449b567d238562b6ff17ee17882986bbe93f0ccc99fff12786fc68638069d32c853b8c7a3ed638303bfd017b8ff103866a7c84f37c4fa9ce119ba3f15e22cdfe5669766ac484dfb282134f560db1c07f4558e1c0caffdcb68ec6582be46535cb04e4d4105140c7520dcaf7cac895393205d30f70bb4d3b25b7697d1f58e0362fb45b9b47eb6e8eb30966ac5c20ec36bc96bfcfa0c9f856a6ef3b417ecd773f19b101645a2554f74197761bb6dc801de436ad1ddaace4ffc51693ba92e42f43cce42db47e9cdb8670e7d12f0d9215825ab24a33a23ade7592ff781b258d49c843c84f5959210f4848166184271f6d64f1501dad901f9d324d93d64532cd03bd073fa8c543566d1fe48dfac1435c3bee588f726451e91f37f5fde0e2e4e4498fdaaa75e98887ab9cab798c0fe38e161d8c5d266b552a03fe4e5671c249fd0fcdde37b1a3b544a227c924886e2f63e41867d70b204e68e27766fcca3cc37b1320efccf3fef75067921d021669b08c601b3465b642a8dc0a4dbd0f7ab756b2dd202333d2a8b3f181a25e002013cb6f9d72c14269f4c9f40a9e0ce454095e65c19826c295ababa70ed6b7b8ab073d9710d3063f0e6d54fda32e982a27ba1daeecca71e2e4dca486358e59a1fdb2b228bb31e0b480064b83d5e03f849a5f9de6fc814da8e2688cfe9e27a087c622df42164fb83b59cc601c72c6fa98d2f79450f9f6469c1c97b7fa4e4b0b06a99dbaad59445bf4b4588b8c12b915a356007da01baf4542732c3c45468983e75004e8d88fc91889ffaf48c715d20121d0fa78bf23d630c1d2a9c3de747115391916901cb0db90b74ec295bb27981a2403169f04839a6a4dd3116beae91b3d32876c7dee09677f929efca2ed0a84f0d07122b47da465b697aef16dedb79ae399b15945ade1a0798d6eaa21449fb8f6bca866bbd360557fa94b96440b8ad0a926ac8eda7c65e1b89a095e1271a3b127142a541305892577c3dd0ae12600bfc2955f7169097540d040911d1419ac1ea9a760e41a658f495c329945afae5d253beed7b68d288c75a5e6f698769cd4c35a7a26abc1d718714c17fb9e219fb645a3be08d147b7475f5a2159631a8046c95733aee627c6dbd07e15d021ce423f437616e294e7027cf56da494f3196269f9966f59690b95a5755ac79655d4a96c1c96eab415bb32655da883aadc47681b5687258455a6b4f045b42c58f5240fe55106f175dde41409920dfdc64520ae4307e3eb94ff02fa14c9f39bbf76b5e40568d4e966f4479604aa682fab5198a387ff6033cf4ebaf3b61f68cd3729b19434b9e03d13c333c5c43c352c82c5f911c66e005dc14b858dba6a3c2b59ffe5207e1578cd61f719156406df6a29e4013b5eb0e20e273eb0e309377e70e20d2fffec05005ff8598b17969feeccc75abd20bfe42d34e751f916d602aa85a91fb861d5a3d75ef6e3205ca41f7f90364e3c289b271fb4ad530f8612142c38651a6cb8d2d7bbba051040000104104000bb4000bb404076815dc82e44b7f427ddc8d563378c5e3b7ed3d8f513b78cdf3879dbc4cd537748de3ae4027589a91b10d33e94070f8d5979319eb09cd174f1614c28108e7737af475bd999f7c6d33f57d95824d4aa04b896859cab3c18cff5ed6beee8e5395a7332ef285ed0f8aef46bdd83a507ff91df6a2ad1361d304dfbad929e333856287ad8e238bd354a8e0d38012c56e7dd2bcd906b04d3e1fead1071615b4b4e6a00cd7ed23577e20e75193df32f1a20e1f019a071a1d59ad484c16dc8dfe8aa665ff0e68914c3542a5108b718fd7516870c391cae2e925bdb031182435a30837373c6244ceecc4493f668759d44d634225562e5086e5c500481a2404454008a2eaf4d427664f1381e2578e12e0173142f98b5874c89f8264c7c3f244a24baed9c6dc450f6b35e6eef5e1a1bd06884ba75eacb16d444b3960c52a29f65ce00414f791c8a2d8fc8598b2879936025d0c8b310a553b042f9548bd8dfc384e278001b35437d18967a4f6e26db1e95eb1ba4fd95ca32372ff55c2a34200460b91fc957b1854f342528ff46155b83d66dd71342d37a8ba4ffe1e97898a5582aa32626cbd88ebf1b82b49ceabbbe40829b6ea32f982ff6bee370022d4a0f6b0e21cb1a548f045c0e8946d60d88f864b4b17819fb2b85bcf33c5a718275f90864e6442b4a95a515a5cae66fc575dcd1969fc9625fe2ede961993809ecbeb9269d653fad0fe4d92f53ab346f6e33985c25e86c3586e86d57a131b04a503ed17e56dc788a1da7ce3cbad9b9cd700fcfe704d985c411d89e323c0cddf313f31677efee5c859356023f0ec6d086846612b23825c8d02a60a9b6693c569cef51caca6138c0b1e515481490398d22047c8a9d6c3a6e35eb6031a1746f2f47d9a913e4e66992f5e0fb0a23911858749bd4a0b08d75afade96d0792335e7bb220ac45e3f8ee8e855579a845b597b1004add2490bb81d7e284b49e7cfd91f6cfd0f312c20caa40b59e2b9663d52a20e30fedc58767582ea04d08d0e8f3a1e1c6863a0f029f353c29009d0d7a0241781803c1c3346a2a20ef713470663143860d6371d8f0112e181428e0729f543718b9b420a08014e0e600c5dc2fe5fe306523e6912a95413e2e0c458bd4621ad23f83242b8ce8cd5e62313927f88a4affe88433e2920a1651a8346cc67c5b95101b34d3cfcc4f3a356aab65fb720e52db5848dec1e295d6b266a06d14c4b5240b048d8ae720c9d0a878f2e1166675e3405432e986057bc362c86a80321028600506dca84e9d632dd3aed60c5bf3920560f1a5236619bdbc62315ec9814c7cc49c258cb2171952066ac03093e61374c7134f3172161e528305c551762f8a6992f51c52e972263093309e1d554cb92610d7c44353a9c9d303a85cfa1b3547d95cc8a839f7c99b47809d73b131e2cd9e33b3272df4eeb340fbe58884127c8621828099af1ddf506232193acdb010a9accc0e909c20c8b01dda22153d4acca74a986ac5415de36d849b20a234f0f29d73247ad2946dac5838f12248cad06982895ca53ceead2bd61e450082ddc100b6754eb1c904afc4ad3263caecd375e36f16d4b1d73468415f6db6db9e03ba90cf34269ac10eccdbc7a130d2e2e4e6362f28d74de89a7c3e3b75009ef008417a930e2bd5b6880f44f69528e0a4d856cd2ec6b78eba4507f0a469ce9c743e921c172446e798ab8ad1f36dd303487c016bb8e380a85a76a22998c23b777563d28c8e839a5b2992538d2155beffb69b61d6a61f7d5f588e14ee24664d3b219ff51dc474cce37b7289c8aef57d04933c0b70b770cfbb60978ece02a78131bb35fae7809248633ee9302d8cbb05382244fe34650f415bfe786cc4c30ff5ed8bace88b2dbbb3ccf4eeb48d39853bd74b39511eedaacb986889361c456f29df49146d3f87bd6d5a548211bb4ea75d14ed808df459aaa53945a5ab2df57926512ef31a2d4bc9e8afe5eccd916505").unwrap();
    // if program_code != raw_data {
    //     bail!(
    //         "program code mismatch, got {} vs {}",
    //         hex::encode(program_code),
    //         hex::encode(raw_data)
    //     );
    // }
    println!("got codehash {:?}", hex::encode(keccak256(&raw_data)));
    let codehash = FixedBytes::<32>::from(keccak256(&raw_data));

    let data = CacheManager::placeBidCall { codehash }.abi_encode();
    let to = H160::from_slice(
        hex::decode("d1bbd579988f394a26d6ec16e77b3fa8a5e8fcee")
            .unwrap()
            .as_slice(),
    );
    let tx = Eip1559TransactionRequest::new()
        .to(NameOrAddress::Address(to))
        .data(data)
        .value(U256::from(args.bid));

    // let privkey = "93690ac9d039285ed00f874a2694d951c1777ac3a165732f36ea773f16179a89".to_string();
    // let wallet = LocalWallet::from_str(&privkey)?;
    // let chain_id = provider.get_chainid().await?.as_u64();
    // let client = Arc::new(SignerMiddleware::new(
    //     provider,
    //     wallet.clone().with_chain_id(chain_id),
    // ));
    // let pending_tx = client.send_transaction(tx, None).await?;
    // let receipt = pending_tx.await?;
    // match receipt {
    //     Some(receipt) => {
    //         println!("Receipt: {:?}", receipt);
    //     }
    //     None => {
    //         bail!("failed to cache program");
    //     }
    // }

    if let Err(EthCallError { data, msg }) =
        eth_call(tx.clone(), State::default(), &provider).await?
    {
        println!("Got data {}, msg {:?}", hex::encode(&data), msg);
        let error = match CacheManager::CacheManagerErrors::abi_decode(&data, true) {
            Ok(err) => err,
            Err(err_details) => bail!("unknown CacheManager error: {msg} and {:?}", err_details),
        };
        use CacheManager::CacheManagerErrors as C;
        match error {
            C::AsmTooLarge(_) => bail!("program too large"),
            _ => bail!("unexpected CacheManager error: {msg}"),
        }
    }

    println!("Succeeded cache call");
    // Otherwise, we are ready to send the tx data if our call passed.
    // TODO: Send.
    Ok(())
}

struct EthCallError {
    data: Vec<u8>,
    msg: String,
}

impl From<EthCallError> for ErrReport {
    fn from(value: EthCallError) -> Self {
        eyre!(value.msg)
    }
}

async fn eth_call(
    tx: Eip1559TransactionRequest,
    mut state: State,
    provider: &Provider<Http>,
) -> Result<Result<Vec<u8>, EthCallError>> {
    let tx = TypedTransaction::Eip1559(tx);
    state.account(Default::default()).balance = Some(ethers::types::U256::MAX); // infinite balance

    match provider.call_raw(&tx).state(&state).await {
        Ok(bytes) => Ok(Ok(bytes.to_vec())),
        Err(ProviderError::JsonRpcClientError(error)) => {
            let error = error
                .as_error_response()
                .ok_or_else(|| eyre!("json RPC failure: {error}"))?;

            let msg = error.message.clone();
            let data = match &error.data {
                Some(Value::String(data)) => cargo_stylus_util::text::decode0x(data)?.to_vec(),
                Some(value) => bail!("failed to decode RPC failure: {value}"),
                None => vec![],
            };
            Ok(Err(EthCallError { data, msg }))
        }
        Err(error) => Err(error.into()),
    }
}
