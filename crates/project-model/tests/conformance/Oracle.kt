// Original synthetic oracle harness. Compile only in an authorized private checkout.
import com.hmiforge.core.binding.*
import com.hmiforge.core.model.*
fun main(args:Array<String>) {
 val types = mapOf("good" to VariableType.DOUBLE,"uncertain" to VariableType.DOUBLE,"bad" to VariableType.DOUBLE,"missing" to VariableType.DOUBLE,"empty" to VariableType.DOUBLE,"nonfinite" to VariableType.DOUBLE,"good-id" to VariableType.DOUBLE,"true" to VariableType.BOOL,"😀" to VariableType.STRING)
 val readings=mapOf("good" to Reading(HmiValue.DoubleValue(3.0)), "uncertain" to Reading(HmiValue.DoubleValue(2.0),Quality.UNCERTAIN),"bad" to Reading(HmiValue.DoubleValue(99.0),Quality.BAD),"empty" to Reading(null),"nonfinite" to Reading(HmiValue.DoubleValue(Double.POSITIVE_INFINITY)),"good-id" to Reading(HmiValue.DoubleValue(4.0)),"true" to Reading(HmiValue.Bool(true)),"😀" to Reading(HmiValue.Text("ok")))
 val vectors=java.io.File(args[0]).readLines().map { String(java.util.Base64.getDecoder().decode(it),Charsets.UTF_8) }
 fun hex(s:String)=s.toByteArray(Charsets.UTF_8).joinToString("") { "%02x".format(it.toInt() and 255) }
 for ((i,s) in vectors.withIndex()){
   val parsed=BindingExpressions.parse(s)
   if(parsed !is ParseResult.Parsed){println("$i\tPARSE");continue}
   val checked=BindingExpressions.check(parsed.expression){types[it]}
   if(checked !is CheckResult.Typed){println("$i\tCHECK");continue}
   val e=BindingExpressions.evaluate(parsed.expression,ValueSource{readings[it]})
   val value=when(val v=e.value){is HmiValue.Bool->"B:"+v.value;is HmiValue.DoubleValue->"N:"+java.lang.Long.toUnsignedString(v.value.toRawBits(),16).padStart(16,'0');is HmiValue.Text->"T:"+hex(v.value);null->"-";else->error("unexpected value")}
   println("$i\tOK\t"+checked.type.name+"\t"+BindingExpressions.variables(parsed.expression).joinToString(",")+"\t"+e.quality.name+"\t"+value+"\t"+(e.error!=null))
 }
}
