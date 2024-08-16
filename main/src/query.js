{
    "hostio": function(info) {
        if (this.nests.includes(info.name)) {
            info.info = this.open.pop();
        }
        this.open.push(info);
    },
    "enter": function(frame) {
        let inner = [];
        this.open.push({
            address: frame.getTo(),
            steps: inner,
        });

        this.stack.push(this.open); // save where we were
        this.open = inner;
    },
    "exit": function(result) {
        this.open = this.stack.pop();
    },
    "result": function() { return this.open; },
    "fault":  function() { return this.open; },
    stack: [],
    open: [],
    nests: ["call_contract", "delegate_call_contract", "static_call_contract"]
}
