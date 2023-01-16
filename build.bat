docker build -t builder . 
docker build -t wgc -f ./wireguard_control.dockerfile .
docker build -t wgb -f ./telegram_bot.dockerfile .