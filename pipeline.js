
function multiply(a, b) {
    return a * b;
}

function reverse(text) {
    return text.split('').reverse().join('');
}

function fibonacci(n) {
    if (n <= 1) return n;
    return fibonacci(n - 1) + fibonacci(n - 2);
}

module.exports = { multiply, reverse, fibonacci };
